# Ad and tracker blocking: reference measurement

Measured 2026-11-06 on Windows 11 with the built-in list plus EasyList and EasyPrivacy (93,011 rules), 12 s after each load, no consent banners accepted.

Requests are counted in WebView2's `WebResourceRequested` event, where Zenith blocks them, so every request of the page is included (all frames, workers, the document). "Off" is the same page reloaded with blocking turned off for that site via the shield in the address pill.

| Site            | Requests, blocking off | Requests, blocking on | Blocked | Requests that reached the network |
| --------------- | ---------------------- | --------------------- | ------- | --------------------------------- |
| cnn.com         | 729                    | 163                   | 52      | 111 (−85% vs off)                 |
| theguardian.com | 28                     | 28                    | 0       | 28 (−0% vs off)                   |
| bbc.com         | 67                     | 34                    | 16      | 18 (−73% vs off)                  |
| imdb.com        | 36                     | 31                    | 7       | 24 (−33% vs off)                  |
| nytimes.com     | 185                    | 56                    | 10      | 46 (−75% vs off)                  |
| reddit.com      | 48                     | 45                    | 2       | 43 (−10% vs off)                  |

Across 6 sites: 1093 requests without blocking; with blocking 87 were blocked and 270 reached the network (−75%).
Blocked scripts also never start the requests they would have made, which is why "blocking on" pages usually make fewer requests in total.

Numbers vary between runs because these sites rotate ads and content.
