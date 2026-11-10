use super::output::reserve;
use super::state::format_clock;
use super::*;
use crate::media::mp4::testkit::{fragment, init};
use crate::media::{hls, mp4};
use std::collections::{HashMap, VecDeque};
use std::io::{self, BufRead, Write};
use std::net::TcpListener;

type Routes = HashMap<String, (String, Vec<u8>)>;
type Log = Arc<Mutex<Vec<(String, String, String)>>>;

/// A tiny HTTP/1.1 server for the engine tests: serves fixed bodies, honours single byte
/// ranges and records the requests it saw (path, range header, cookie header).
struct Server {
    base: String,
    log: Log,
}

fn serve(routes: Routes) -> Server {
    serve_seq(routes, HashMap::new())
}

/// `seqs`: replies served in turn for a path (the last one repeats). A content type of
/// "redirect" sends the body as the Location; "status/NNN" answers with that status.
fn serve_seq(routes: Routes, seqs: HashMap<String, Vec<(String, Vec<u8>)>>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let routes = Arc::new(routes);
    let seqs: HashMap<String, VecDeque<(String, Vec<u8>)>> = seqs.into_iter().map(|(k, v)| (k, v.into())).collect();
    let seqs = Arc::new(Mutex::new(seqs));
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let log2 = log.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let routes = routes.clone();
            let seqs = seqs.clone();
            let log = log2.clone();
            std::thread::spawn(move || {
                let mut reader = io::BufReader::new(stream.try_clone().unwrap());
                let mut stream = stream;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                    let (mut range, mut cookie) = (String::new(), String::new());
                    loop {
                        let mut h = String::new();
                        if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                            break;
                        }
                        let lower = h.to_ascii_lowercase();
                        if let Some(v) = lower.strip_prefix("range:") {
                            range = v.trim().to_string();
                        } else if lower.starts_with("cookie:") {
                            cookie = h["cookie:".len()..].trim().to_string();
                        }
                    }
                    lock(&log).push((path.clone(), range.clone(), cookie));
                    let key = path.split('?').next().unwrap_or("").to_string();
                    let next = {
                        let mut s = lock(&seqs);
                        match s.get_mut(&key) {
                            Some(q) if q.len() > 1 => q.pop_front(),
                            Some(q) => q.front().cloned(),
                            None => None,
                        }
                    };
                    let route = next.or_else(|| routes.get(&key).cloned());
                    let (status, ctype, body, extra) = match route {
                        None => ("404 Not Found".to_string(), "text/plain".to_string(), b"missing".to_vec(), String::new()),
                        Some((ct, body)) if ct == "redirect" => {
                            let location = format!("Location: {}\r\n", String::from_utf8_lossy(&body));
                            ("302 Found".to_string(), "text/plain".to_string(), Vec::new(), location)
                        }
                        Some((ct, _)) if ct.starts_with("status/") => {
                            (format!("{} Error", &ct[7..]), "text/plain".to_string(), Vec::new(), String::new())
                        }
                        Some((ct, body)) => match range.strip_prefix("bytes=").and_then(|r| r.split_once('-')) {
                            Some((a, b)) => {
                                let a: usize = a.parse().unwrap();
                                let b: usize = if b.is_empty() { body.len() - 1 } else { b.parse::<usize>().unwrap().min(body.len() - 1) };
                                let extra = format!("Content-Range: bytes {a}-{b}/{}\r\n", body.len());
                                ("206 Partial Content".to_string(), ct, body[a..=b].to_vec(), extra)
                            }
                            None => ("200 OK".to_string(), ct, body, String::new()),
                        },
                    };
                    let head = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\n{extra}Connection: keep-alive\r\n\r\n",
                        body.len()
                    );
                    if stream.write_all(head.as_bytes()).is_err() || stream.write_all(&body).is_err() {
                        return;
                    }
                }
            });
        }
    });
    Server { base, log }
}

struct TestEnv;
impl Env for TestEnv {
    fn cookies(&self, _url: &str) -> Option<String> {
        Some("session=abc".into())
    }
    fn ffmpeg(&self) -> Option<PathBuf> {
        None
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("zenith-job-test-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// Cookies that name the origin they are for, to see which host received which.
struct OriginCookies;
impl Env for OriginCookies {
    fn cookies(&self, url: &str) -> Option<String> {
        Some(format!("site={}", tauri::Url::parse(url).ok()?.port()?))
    }
    fn ffmpeg(&self) -> Option<PathBuf> {
        None
    }
}

/// Runs a plan with a given environment; `watch` runs alongside (it may press Stop).
fn run_with(
    plan: Plan,
    name: &str,
    env: &dyn Env,
    live_wait: Option<Duration>,
    watch: impl FnOnce(&Control, &Progress) + Send,
) -> (Result<Output, Halt>, PathBuf) {
    let root = temp_dir(name);
    let (work, out) = (root.join("work"), root.join("out"));
    fs::create_dir_all(&work).unwrap();
    let client = Client::new("ZenithTest/1.0").unwrap();
    let headers = vec![("Referer".to_string(), "https://page.example/watch".to_string())];
    let (control, progress) = (Control::default(), Progress::default());
    let result = std::thread::scope(|s| {
        s.spawn(|| watch(&control, &progress));
        let mut ctx = Ctx::new(&client, &headers, env, &control, &progress, &work);
        ctx.live_wait = live_wait;
        let mut job = Job::new(plan, &work);
        job.run(&ctx).and_then(|()| job.finish(&ctx, &out, "Clip"))
    });
    (result, root)
}

fn run_plan(plan: Plan, name: &str, prepare: impl FnOnce(&Path)) -> (Result<Output, Halt>, PathBuf) {
    let root = temp_dir(name);
    let (work, out) = (root.join("work"), root.join("out"));
    fs::create_dir_all(&work).unwrap();
    prepare(&work);
    let client = Client::new("ZenithTest/1.0").unwrap();
    let headers = vec![("Referer".to_string(), "https://page.example/watch".to_string())];
    let (env, control, progress) = (TestEnv, Control::default(), Progress::default());
    let ctx = Ctx::new(&client, &headers, &env, &control, &progress, &work);
    let mut job = Job::new(plan, &work);
    let result = job.run(&ctx).and_then(|()| job.finish(&ctx, &out, "Clip"));
    (result, root)
}

#[test]
fn hls_transport_stream_with_byte_ranges() {
    let body: Vec<u8> = (0..30_000u32).map(|i| (i % 251) as u8).collect();
    let playlist = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-PLAYLIST-TYPE:VOD\n\
            #EXTINF:4,\n#EXT-X-BYTERANGE:10000@0\nall.ts\n\
            #EXTINF:4,\n#EXT-X-BYTERANGE:10000\nall.ts\n\
            #EXTINF:4,\n#EXT-X-BYTERANGE:10000\nall.ts\n#EXT-X-ENDLIST\n";
    let mut routes = Routes::new();
    routes.insert("/v/index.m3u8".into(), ("application/vnd.apple.mpegurl".into(), playlist.as_bytes().to_vec()));
    routes.insert("/v/all.ts".into(), ("video/mp2t".into(), body.clone()));
    let server = serve(routes);
    let plan = Plan { main: Source::Hls(format!("{}/v/index.m3u8", server.base)), audio: None, live: false };
    let (result, root) = run_plan(plan, "ts", |_| {});
    let out = result.unwrap();
    assert_eq!(out.files.len(), 1);
    assert_eq!(out.files[0].file_name().unwrap(), "Clip.ts");
    assert_eq!(fs::read(&out.files[0]).unwrap(), body);
    let log = lock(&server.log).clone();
    assert!(log.iter().any(|(p, r, _)| p == "/v/all.ts" && r == "bytes=10000-19999"), "{log:?}");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn hls_fmp4_with_separate_audio_is_merged() {
    let mut routes = Routes::new();
    let video_pl = "#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXT-X-MAP:URI=\"v/init.mp4\"\n\
            #EXTINF:2,\nv/0.m4s\n#EXTINF:2,\nv/1.m4s\n#EXTINF:2,\nv/2.m4s\n#EXT-X-ENDLIST\n";
    let mut audio_pl = String::from("#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-MAP:URI=\"a/init.mp4\"\n");
    for i in 0..6 {
        audio_pl.push_str(&format!("#EXTINF:1,\na/{i}.m4s\n"));
    }
    audio_pl.push_str("#EXT-X-ENDLIST\n");
    routes.insert("/s/video.m3u8".into(), ("application/x-mpegurl".into(), video_pl.as_bytes().to_vec()));
    routes.insert("/s/audio.m3u8".into(), ("application/x-mpegurl".into(), audio_pl.into_bytes()));
    routes.insert("/s/v/init.mp4".into(), ("video/mp4".into(), init(1, 90000, b"avc1", &[])));
    routes.insert("/s/a/init.mp4".into(), ("audio/mp4".into(), init(1, 48000, b"mp4a", &[])));
    for i in 0..3u64 {
        routes.insert(format!("/s/v/{i}.m4s"), ("video/mp4".into(), fragment(1, 1, i * 180_000, 180_000, &[b'V', i as u8])));
    }
    for i in 0..6u64 {
        routes.insert(format!("/s/a/{i}.m4s"), ("audio/mp4".into(), fragment(1, 1, i * 48_000, 48_000, &[b'A', i as u8])));
    }
    let server = serve(routes);
    let plan = Plan {
        main: Source::Hls(format!("{}/s/video.m3u8", server.base)),
        audio: Some(Source::Hls(format!("{}/s/audio.m3u8", server.base))),
        live: false,
    };
    let (result, root) = run_plan(plan, "fmp4", |_| {});
    let out = result.unwrap();
    assert_eq!(out.files.len(), 1, "merged into one file: {}", out.note);
    assert_eq!(out.files[0].extension().unwrap(), "mp4");
    let m = fs::read(&out.files[0]).unwrap();
    let top = mp4::children(&m, 0, m.len());
    assert_eq!(top.iter().filter(|b| &b.kind == b"moof").count(), 9);
    let moov = top.iter().find(|b| &b.kind == b"moov").unwrap();
    assert_eq!(mp4::children(&m, moov.body, moov.end).iter().filter(|b| &b.kind == b"trak").count(), 2);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn dash_segments_are_appended_in_order() {
    let mut routes = Routes::new();
    let mut media = Vec::new();
    let mut expected = b"INIT".to_vec();
    routes.insert("/d/init.mp4".into(), ("video/mp4".into(), b"INIT".to_vec()));
    for i in 0..40u32 {
        let body = format!("seg{i:03};").into_bytes();
        expected.extend_from_slice(&body);
        routes.insert(format!("/d/{i}.m4s"), ("video/iso.segment".into(), body));
        media.push(SegRef { url: String::new(), range: None });
    }
    let server = serve(routes);
    for (i, m) in media.iter_mut().enumerate() {
        m.url = format!("{}/d/{i}.m4s", server.base);
    }
    let init = Some(SegRef { url: format!("{}/d/init.mp4", server.base), range: None });
    let plan = Plan { main: Source::Segments { init, media, ext: "mp4".into() }, audio: None, live: false };
    let (result, root) = run_plan(plan, "dash", |_| {});
    let out = result.unwrap();
    assert_eq!(fs::read(&out.files[0]).unwrap(), expected);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn files_resume_from_partial_data() {
    let body: Vec<u8> = (0..200_000u32).map(|i| (i * 7 % 256) as u8).collect();
    let mut routes = Routes::new();
    routes.insert("/f/movie.mp4".into(), ("video/mp4".into(), body.clone()));
    let server = serve(routes);
    let plan = Plan {
        main: Source::File { url: format!("{}/f/movie.mp4", server.base), ext: "mp4".into() },
        audio: None,
        live: false,
    };
    let prefix = body[..50_000].to_vec();
    let (result, root) = run_plan(plan, "file", move |work| fs::write(work.join("track0.part"), &prefix).unwrap());
    let out = result.unwrap();
    assert_eq!(fs::read(&out.files[0]).unwrap(), body);
    assert!(lock(&server.log).iter().any(|(_, r, _)| r == "bytes=50000-"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn encrypted_and_missing_streams_fail_cleanly() {
    let mut routes = Routes::new();
    routes.insert(
        "/e/index.m3u8".into(),
        (
            "application/vnd.apple.mpegurl".into(),
            b"#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-KEY:METHOD=AES-128,URI=\"k\"\n#EXTINF:4,\n0.ts\n#EXT-X-ENDLIST\n".to_vec(),
        ),
    );
    let server = serve(routes);
    let plan = Plan { main: Source::Hls(format!("{}/e/index.m3u8", server.base)), audio: None, live: false };
    let (result, root) = run_plan(plan, "enc", |_| {});
    assert_eq!(result.unwrap_err(), Halt::Failed(PROTECTED.into()));
    let _ = fs::remove_dir_all(root);

    let plan = Plan { main: Source::Hls(format!("{}/missing.m3u8", server.base)), audio: None, live: false };
    let (result, root) = run_plan(plan, "missing", |_| {});
    assert!(matches!(result, Err(Halt::Failed(m)) if m.contains("no longer available")));
    let _ = fs::remove_dir_all(root);
}

fn content_type_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "m3u8" => "application/vnd.apple.mpegurl",
        "mpd" => "application/dash+xml",
        "ts" => "video/mp2t",
        "mp4" | "m4s" => "video/mp4",
        _ => "application/octet-stream",
    }
}

struct RealEnv(Option<PathBuf>);
impl Env for RealEnv {
    fn cookies(&self, _url: &str) -> Option<String> {
        None
    }
    fn ffmpeg(&self) -> Option<PathBuf> {
        self.0.clone()
    }
}

/// End to end on real streams made by FFmpeg: serves ZENITH_MEDIA_DIR over HTTP, reads
/// ZENITH_MEDIA_ENTRY (master playlist, media playlist or MPD) the way the media panel does,
/// downloads the default (best) choice into ZENITH_MEDIA_OUT. ZENITH_FFMPEG optionally enables
/// FFmpeg conversion. Run with `--ignored --nocapture` and check the result with ffprobe.
#[test]
#[ignore]
fn real_stream_from_env() {
    let var = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("set {k}"));
    let dir = PathBuf::from(var("ZENITH_MEDIA_DIR"));
    let entry = var("ZENITH_MEDIA_ENTRY");
    let out = PathBuf::from(var("ZENITH_MEDIA_OUT"));
    let mut routes = Routes::new();
    let mut stack = vec![dir.clone()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p.strip_prefix(&dir).unwrap().to_string_lossy().replace('\\', "/");
                routes.insert(format!("/{rel}"), (content_type_for(&rel).to_string(), fs::read(&p).unwrap()));
            }
        }
    }
    let server = serve(routes);
    let url = format!("{}/{entry}", server.base);
    let client = Client::new("ZenithTest/1.0").unwrap();
    let (text, base) = client.text(&url, &|_| Vec::new(), 1 << 24).unwrap();
    let found = || crate::media::Found { by: "network", headers: vec![], page_url: "https://page.example/", title: "Real stream" };
    let mut media = crate::media::TabMedia::default();
    if entry.ends_with(".mpd") {
        media.add_dash(&url, crate::media::dash::parse(&text, &base).unwrap(), found());
    } else {
        media.add_hls(&url, hls::parse(&text, &base).unwrap(), found());
    }
    let item = media.items().remove(0);
    println!("item: {} {:?} variants={:?} audio={:?}", item.kind, item.duration, item.variants.iter().map(|v| &v.label).collect::<Vec<_>>(), item.audio_tracks.len());
    let req = media.request("t", item.id, None, None, "ZenithTest/1.0").unwrap();
    println!("plan: {:?}", req.plan);
    let work = out.join("work");
    fs::create_dir_all(&work).unwrap();
    let env = RealEnv(std::env::var_os("ZENITH_FFMPEG").map(PathBuf::from));
    let (control, progress) = (Control::default(), Progress::default());
    let ctx = Ctx::new(&client, &req.headers, &env, &control, &progress, &work);
    let mut job = Job::new(req.plan.clone(), &work);
    job.run(&ctx).unwrap();
    let output = job.finish(&ctx, &out, &req.stem).unwrap();
    println!("output: {output:?} progress: {:?}", progress.summary());
}

#[test]
fn redirects_give_each_host_its_own_cookies() {
    let body: Vec<u8> = (0..100_000u32).map(|i| (i % 256) as u8).collect();
    let mut b_routes = Routes::new();
    b_routes.insert("/clip.mp4".into(), ("video/mp4".into(), body.clone()));
    let b = serve(b_routes);
    let mut a_routes = Routes::new();
    a_routes.insert("/r/clip.mp4".into(), ("redirect".into(), format!("{}/clip.mp4", b.base).into_bytes()));
    let a = serve(a_routes);
    let plan = Plan { main: Source::File { url: format!("{}/r/clip.mp4", a.base), ext: "mp4".into() }, audio: None, live: false };
    let (result, root) = run_with(plan, "redirect", &OriginCookies, None, |_, _| {});
    let out = result.unwrap();
    assert_eq!(fs::read(&out.files[0]).unwrap(), body);
    let port = |base: &str| base.rsplit(':').next().unwrap().to_string();
    let (a_log, b_log) = (lock(&a.log).clone(), lock(&b.log).clone());
    assert!(a_log.iter().all(|(_, _, c)| *c == format!("site={}", port(&a.base))), "{a_log:?}");
    assert!(!b_log.is_empty() && b_log.iter().all(|(_, _, c)| *c == format!("site={}", port(&b.base))), "{b_log:?}");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn the_address_policy_applies_to_every_redirect() {
    let mut b_routes = Routes::new();
    b_routes.insert("/x".into(), ("video/mp4".into(), vec![1, 2, 3]));
    let b = serve(b_routes);
    let mut a_routes = Routes::new();
    a_routes.insert("/r".into(), ("redirect".into(), format!("{}/x", b.base).into_bytes()));
    let a = serve(a_routes);
    let b_base = b.base.clone();
    let client = Client::with_policy("ZenithTest/1.0", Box::new(move |u: &str| !u.starts_with(&b_base))).unwrap();
    let r = client.get(&format!("{}/r", a.base), &|_| Vec::new(), None, false);
    assert_eq!(r.err(), Some(http::Error::Blocked));
    assert!(lock(&b.log).is_empty(), "the blocked host was never contacted");
    // The real policy: a public page may not reach the local network; a local page may.
    assert!(!network_policy("https://news.example/story")("http://127.0.0.1:8080/v.mp4"));
    assert!(!network_policy("https://news.example/story")("http://192.168.1.1/admin"));
    assert!(network_policy("https://news.example/story")("https://cdn.example/v.mp4"));
    assert!(network_policy("http://192.168.1.20/library")("http://192.168.1.20/v.mp4"));
}

fn live_playlist(first: u64, last: u64, ended: bool) -> (String, Vec<u8>) {
    let mut s = format!("#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXT-X-MEDIA-SEQUENCE:{first}\n");
    for n in first..last {
        s.push_str(&format!("#EXTINF:2,\ns{n}.ts\n"));
    }
    if ended {
        s.push_str("#EXT-X-ENDLIST\n");
    }
    ("application/vnd.apple.mpegurl".to_string(), s.into_bytes())
}

#[test]
fn a_recording_is_kept_when_the_stream_goes_away() {
    let mut routes = Routes::new();
    let mut expected = Vec::new();
    for n in 0..4u64 {
        let body = format!("seg{n};").into_bytes();
        expected.extend_from_slice(&body);
        routes.insert(format!("/live/s{n}.ts"), ("video/mp2t".into(), body));
    }
    // Two refreshes, then the playlist is gone for good (no EXT-X-ENDLIST).
    let mut seqs = HashMap::new();
    seqs.insert(
        "/live/index.m3u8".to_string(),
        vec![live_playlist(0, 3, false), live_playlist(1, 4, false), ("status/404".to_string(), Vec::new())],
    );
    let server = serve_seq(routes, seqs);
    let plan = Plan { main: Source::Hls(format!("{}/live/index.m3u8", server.base)), audio: None, live: true };
    let (result, root) = run_with(plan, "live-gone", &TestEnv, Some(Duration::from_millis(10)), |_, _| {});
    let out = result.unwrap();
    assert_eq!(fs::read(&out.files[0]).unwrap(), expected, "each segment once, in order");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn stopping_a_live_recording_saves_it() {
    let mut routes = Routes::new();
    let mut expected = Vec::new();
    for n in 7..10u64 {
        let body = format!("live{n};").into_bytes();
        expected.extend_from_slice(&body);
        routes.insert(format!("/l/s{n}.ts"), ("video/mp2t".into(), body));
    }
    routes.insert("/l/index.m3u8".into(), live_playlist(7, 10, false));
    let server = serve(routes);
    let plan = Plan { main: Source::Hls(format!("{}/l/index.m3u8", server.base)), audio: None, live: true };
    let (result, root) = run_with(plan, "live-stop", &TestEnv, Some(Duration::from_millis(10)), |control, progress| {
        let until = Instant::now() + Duration::from_secs(10);
        while progress.tracks[0].done.load(Ordering::Relaxed) < 3 && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        control.set(STOP);
    });
    let out = result.unwrap();
    assert_eq!(fs::read(&out.files[0]).unwrap(), expected);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn names_are_unique() {
    let dir = temp_dir("names");
    let a = reserve(&dir, "Video", "mp4").unwrap();
    let b = reserve(&dir, "Video", "mp4").unwrap();
    assert_eq!(a.file_name().unwrap(), "Video.mp4");
    assert_eq!(b.file_name().unwrap(), "Video (1).mp4");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn progress_estimates_totals_from_segments() {
    let p = Progress::default();
    // Two of ten segments appended (800 bytes); workers already hold 200 more.
    p.tracks[0].bytes.store(1000, Ordering::Relaxed);
    p.tracks[0].appended.store(800, Ordering::Relaxed);
    p.tracks[0].done.store(2, Ordering::Relaxed);
    p.tracks[0].count.store(10, Ordering::Relaxed);
    p.tracks[1].bytes.store(300, Ordering::Relaxed);
    p.tracks[1].total.store(900, Ordering::Relaxed);
    let s = p.summary();
    assert_eq!(s.bytes, 1300);
    assert_eq!(s.total, 4000 + 900, "estimated from appended segments, not in-flight bytes");
    assert_eq!((s.done, s.count), (2, 10));
    assert_eq!(format_clock(3_725_000), "1:02:05");
    assert_eq!(format_clock(65_000), "1:05");
}

/// Runs a plan in fast mode with `connections` per file; returns the output and progress.
fn run_fast(plan: Plan, name: &str, connections: usize) -> (Result<Output, Halt>, bool, PathBuf) {
    let root = temp_dir(name);
    let (work, out) = (root.join("work"), root.join("out"));
    fs::create_dir_all(&work).unwrap();
    let client = Client::new("ZenithTest/1.0").unwrap();
    let headers = vec![("Referer".to_string(), "https://page.example/files".to_string())];
    let (env, control, progress) = (TestEnv, Control::default(), Progress::default());
    let mut ctx = Ctx::new(&client, &headers, &env, &control, &progress, &work);
    ctx.connections = connections;
    let mut job = Job::new(plan, &work);
    let result = job.run(&ctx).and_then(|()| job.finish(&ctx, &out, "setup"));
    (result, progress.fast.load(Ordering::Relaxed), root)
}

fn big_body(len: usize) -> Vec<u8> {
    (0..len as u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect()
}

#[test]
fn fast_mode_splits_a_file_over_several_connections() {
    let body = big_body(9 << 20);
    let mut routes = Routes::new();
    routes.insert("/dl/setup.exe".into(), ("application/octet-stream".into(), body.clone()));
    let server = serve(routes);
    let url = format!("{}/dl/setup.exe", server.base);
    let plan = Plan { main: Source::Download { url, ext: "exe".into() }, audio: None, live: false };
    let (result, fast, root) = run_fast(plan, "fast-split", 4);
    let out = result.unwrap();
    assert_eq!(out.files[0].file_name().unwrap(), "setup.exe");
    assert!(fs::read(&out.files[0]).unwrap() == body, "pieces reassembled in place");
    assert!(fast);
    let log = lock(&server.log).clone();
    assert_eq!(log[0].1, "bytes=0-0", "probed first");
    let pieces: Vec<_> = log.iter().skip(1).map(|(_, r, _)| r.clone()).collect();
    // 9 MiB for 4 connections: pieces of 1 MiB (the minimum), each asked for exactly.
    assert_eq!(pieces.len(), 9, "{pieces:?}");
    assert!(pieces.contains(&"bytes=0-1048575".to_string()), "{pieces:?}");
    assert!(pieces.contains(&format!("bytes={}-{}", 8 << 20, (9 << 20) - 1)), "{pieces:?}");
    assert!(log.iter().all(|(_, _, c)| c == "session=abc"), "every connection sends the cookies");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn fast_mode_leaves_small_files_and_plain_downloads_alone() {
    let small = big_body(300_000);
    let ts = big_body(5 << 20);
    let mut routes = Routes::new();
    routes.insert("/small.bin".into(), ("application/octet-stream".into(), small.clone()));
    routes.insert("/rec.ts".into(), ("video/mp2t".into(), ts.clone()));
    routes.insert("/README".into(), ("text/plain".into(), small.clone()));
    let server = serve(routes);

    let plan = Plan { main: Source::File { url: format!("{}/small.bin", server.base), ext: "bin".into() }, audio: None, live: false };
    let (result, fast, root) = run_fast(plan, "fast-small", 8);
    assert!(fs::read(&result.unwrap().files[0]).unwrap() == small);
    assert!(!fast, "too small to split");
    let ranges: Vec<_> = lock(&server.log).iter().map(|(_, r, _)| r.clone()).collect();
    assert_eq!(ranges, vec!["bytes=0-0".to_string(), String::new()], "probe, then one plain request");
    let _ = fs::remove_dir_all(root);

    // A page's .ts download stays a .ts file, and a name without an extension keeps none.
    let plan = Plan { main: Source::Download { url: format!("{}/rec.ts", server.base), ext: "ts".into() }, audio: None, live: false };
    let (result, _, root) = run_fast(plan, "fast-ts", 8);
    let out = result.unwrap();
    assert_eq!(out.files[0].file_name().unwrap(), "setup.ts");
    assert!(fs::read(&out.files[0]).unwrap() == ts);
    let _ = fs::remove_dir_all(root);
    let plan = Plan { main: Source::Download { url: format!("{}/README", server.base), ext: String::new() }, audio: None, live: false };
    let (result, _, root) = run_fast(plan, "fast-noext", 8);
    assert_eq!(result.unwrap().files[0].file_name().unwrap(), "setup");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn fast_mode_pieces_cover_the_file() {
    for (total, connections) in [(4u64 << 20, 8usize), ((10 << 30) + 7, 8), ((5 << 20) + 3, 2), (1, 8)] {
        let pieces = super::ranged::split(total, connections);
        assert_eq!(pieces.first().unwrap().start, 0);
        assert_eq!(pieces.last().unwrap().end, total);
        assert!(pieces.windows(2).all(|w| w[0].end == w[1].start));
        assert!(pieces.iter().all(|p| p.end - p.start <= 64 << 20));
    }
    assert_eq!(super::ranged::split(10 << 30, 8).len(), 160, "64 MiB pieces for a 10 GiB file");
}
