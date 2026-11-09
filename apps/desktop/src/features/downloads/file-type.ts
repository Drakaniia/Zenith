// What kind of file a download is, for its icon: a glyph, a colour and the extension, so a PDF,
// a Word document, a picture or an archive can be told apart at a glance.

import type { IconName } from "../../shared/ui/icons";

export type FileKind =
  | "pdf"
  | "word"
  | "sheet"
  | "slides"
  | "image"
  | "video"
  | "audio"
  | "archive"
  | "code"
  | "text"
  | "installer"
  | "other";

export interface FileType {
  kind: FileKind;
  icon: IconName;
  /** The glyph's colour; `null` keeps the surrounding text colour. */
  color: string | null;
  /** The extension in capitals, at most four letters ("" when there is none). */
  label: string;
}

const KINDS: Record<FileKind, { icon: IconName; color: string | null }> = {
  pdf: { icon: "fileText", color: "#e5484d" },
  word: { icon: "fileText", color: "#3b82f6" },
  sheet: { icon: "fileSheet", color: "#1f9d55" },
  slides: { icon: "fileSlides", color: "#f76b15" },
  image: { icon: "image", color: "#46a758" },
  video: { icon: "film", color: "#8e4ec6" },
  audio: { icon: "music", color: "#d6409f" },
  archive: { icon: "fileZip", color: "#e2a336" },
  code: { icon: "fileCode", color: "#12a5c3" },
  text: { icon: "fileText", color: null },
  installer: { icon: "fileApp", color: "#6e6ade" },
  other: { icon: "file", color: null },
};

const BY_EXT: [FileKind, RegExp][] = [
  ["pdf", /^pdf$/],
  ["word", /^(docx?|docm|dotx?|odt|rtf|pages)$/],
  ["sheet", /^(xlsx?|xlsm|xltx?|ods|csv|tsv|numbers)$/],
  ["slides", /^(pptx?|pptm|potx?|odp|key)$/],
  ["image", /^(png|jpe?g|gif|webp|avif|svg|bmp|ico|heic|heif|tiff?)$/],
  ["video", /^(mp4|mkv|webm|mov|avi|m4v|ts|wmv|flv)$/],
  ["audio", /^(mp3|m4a|aac|flac|wav|ogg|opus|wma)$/],
  ["archive", /^(zip|rar|7z|tar|gz|tgz|bz2|xz|zst|cab|iso)$/],
  ["installer", /^(exe|msi|msix|msixbundle|appx|appxbundle)$/],
  ["code", /^(js|mjs|ts|json|html?|css|py|rs|java|c|cpp|h|cs|go|rb|php|sh|ps1|bat|xml|ya?ml|toml)$/],
  ["text", /^(txt|md|log|ini|cfg)$/],
];

const BY_MIME: [FileKind, RegExp][] = [
  ["pdf", /^application\/pdf$/],
  ["word", /wordprocessingml|msword|opendocument\.text/],
  ["sheet", /spreadsheetml|ms-excel|opendocument\.spreadsheet|^text\/csv$/],
  ["slides", /presentationml|ms-powerpoint|opendocument\.presentation/],
  ["image", /^image\//],
  ["video", /^video\//],
  ["audio", /^audio\//],
  ["archive", /zip|x-rar|x-7z|x-tar|gzip/],
  ["installer", /x-msdownload|x-msi|x-ms-installer/],
  ["text", /^text\/plain$/],
];

/** The extension of a file name or path, lower case ("" for none or a dot file). */
export function extOf(name: string): string {
  const base = name.split(/[\\/]/).pop() ?? "";
  const dot = base.lastIndexOf(".");
  return dot > 0 && dot < base.length - 1 ? base.slice(dot + 1).toLowerCase() : "";
}

/** The kind of a file by its extension, else by its MIME type. */
export function fileType(name: string, mime = ""): FileType {
  const ext = extOf(name);
  const m = mime.toLowerCase();
  const kind =
    (ext && BY_EXT.find(([, re]) => re.test(ext))?.[0]) ||
    (m && BY_MIME.find(([, re]) => re.test(m))?.[0]) ||
    "other";
  return { kind, ...KINDS[kind], label: ext.slice(0, 4).toUpperCase() };
}
