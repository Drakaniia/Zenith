// Profile pictures: the bundled emoji (see profile-avatars.ts), what an avatar shows, and the
// canvas work that turns an emoji or a chosen picture into the 256 px PNG the backend keeps for
// the window and desktop-shortcut icons.
import { PROFILE_EMOJI, type ProfileEmoji } from "./generated/profile-avatars";
import { MAX_PICTURE_BYTES, PROFILE_ICON_SIZE, centerSquare, emojiName, profileInitial } from "./profiles";
import type { ProfileInfo } from "../../shared/types";

/** How much of the circle an emoji covers. */
const EMOJI_SCALE = 0.62;

const EMOJI = new Map(PROFILE_EMOJI.map((e) => [e.id, e]));

/** An emoji's SVG as an `<img>` source. */
export function emojiSrc(emoji: ProfileEmoji, size?: number): string {
  const svg = size ? emoji.svg.replace("<svg ", `<svg width="${size}" height="${size}" `) : emoji.svg;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

/** The emoji offered in the profile dialog, with their image sources. */
export const EMOJI_CHOICES = PROFILE_EMOJI.map((e) => ({ ...e, avatar: `emoji:${e.id}`, src: emojiSrc(e) }));

/** The bundled emoji of an `emoji:<name>` avatar, if it is one Zenith ships. */
export function avatarEmoji(avatar: string | null | undefined): ProfileEmoji | null {
  const name = emojiName(avatar);
  return (name && EMOJI.get(name)) || null;
}

/** What a profile's avatar shows: its picture, a bundled emoji on its colour, or its initial. */
export type AvatarFace =
  | { kind: "image"; src: string }
  | { kind: "emoji"; src: string; label: string }
  | { kind: "letter"; text: string };

export function avatarFace(p: Pick<ProfileInfo, "name" | "avatar" | "image">): AvatarFace {
  if (p.avatar === "image" && p.image) return { kind: "image", src: p.image };
  const emoji = avatarEmoji(p.avatar);
  if (emoji) return { kind: "emoji", src: emojiSrc(emoji), label: emoji.label };
  return { kind: "letter", text: profileInitial(p.name) };
}

function iconCanvas(): [HTMLCanvasElement, CanvasRenderingContext2D] {
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = PROFILE_ICON_SIZE;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("Couldn't draw the profile picture.");
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";
  // Everything is drawn inside the round avatar; the corners stay transparent.
  const r = PROFILE_ICON_SIZE / 2;
  ctx.beginPath();
  ctx.arc(r, r, r, 0, Math.PI * 2);
  ctx.closePath();
  ctx.clip();
  return [canvas, ctx];
}

async function loadImage(src: string): Promise<HTMLImageElement> {
  const img = new Image();
  img.src = src;
  await img.decode();
  return img;
}

/** An emoji on a circle of `color`, as a PNG data URL. */
export async function renderEmojiIcon(emoji: ProfileEmoji, color: string): Promise<string> {
  const size = Math.round(PROFILE_ICON_SIZE * EMOJI_SCALE);
  const img = await loadImage(emojiSrc(emoji, size));
  const [canvas, ctx] = iconCanvas();
  ctx.fillStyle = color;
  ctx.fillRect(0, 0, PROFILE_ICON_SIZE, PROFILE_ICON_SIZE);
  const at = (PROFILE_ICON_SIZE - size) / 2;
  ctx.drawImage(img, at, at, size, size);
  return canvas.toDataURL("image/png");
}

/**
 * A picture file the user chose, cropped to its centred square and scaled to the round 256 px
 * avatar, as a PNG data URL. Throws an error with a message for the user.
 */
export async function pictureFromFile(file: File): Promise<string> {
  if (file.type && !file.type.startsWith("image/")) throw new Error("That file isn't a picture.");
  if (file.size > MAX_PICTURE_BYTES) throw new Error("That picture is too large. Choose one under 25 MB.");
  let bitmap: ImageBitmap;
  try {
    bitmap = await createImageBitmap(file);
  } catch {
    throw new Error("That picture couldn't be opened. Try a PNG, JPEG or WebP file.");
  }
  try {
    if (!bitmap.width || !bitmap.height) throw new Error("That picture is empty.");
    const { x, y, size } = centerSquare(bitmap.width, bitmap.height);
    const [canvas, ctx] = iconCanvas();
    ctx.drawImage(bitmap, x, y, size, size, 0, 0, PROFILE_ICON_SIZE, PROFILE_ICON_SIZE);
    return canvas.toDataURL("image/png");
  } finally {
    bitmap.close();
  }
}
