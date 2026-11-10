import { Buffer } from "buffer";
import { mkdir, readdir, rmdir, unlink, writeFile } from "fs/promises";

// Generated silence, 60 seconds, mono FLAC. No external file or audio encoder is needed.
const silence = "ZkxhQwAAACISABIAAAALAAANAfQA8AAHUwCrqrfAf8Xf2T5SA4TfwF6ahAAANgUAAABBcHBsZQEAAAAlAAAAV0FWRUZPUk1BVEVYVEVOU0lCTEVfQ0hBTk5FTF9NQVNLPTB4NP/4VAgArQAAANVn//hUCAGqAAAAuR//+FQIAqMAAAANl//4VAgDpAAAAGHv//hUCASxAAAA5IL/+FQIBbYAAACI+v/4VAgGvwAAADxy//hUCAe4AAAAUAr/+FQICJUAAAC2rf/4VAgJkgAAANrV//hUCAqbAAAAbl3/+FQIC5wAAAACJf/4VAgMiQAAAIdI//hUCA2OAAAA6zD/+FQIDocAAABfuP/4VAgPgAAAADPA//hUCBDdAAAAEvP/+FQIEdoAAAB+i//4VAgS0wAAAMoD//hUCBPUAAAApnv/+FQIFMEAAAAjFv/4VAgVxgAAAE9u//hUCBbPAAAA++b/+FQIF8gAAACXnv/4VAgY5QAAAHE5//hUCBniAAAAHUH/+FQIGusAAACpyf/4VAgb7AAAAMWx//hUCBz5AAAAQNz/+FQIHf4AAAAspP/4VAge9wAAAJgs//hUCB/wAAAA9FT/+FQIIE0AAADaSv/4VAghSgAAALYy//hUCCJDAAAAArr/+FQII0QAAABuwv/4VAgkUQAAAOuv//hUCCVWAAAAh9f/+FQIJl8AAAAzX//4VAgnWAAAAF8n//hUCCh1AAAAuYD/+FQIKXIAAADV+P/4VAgqewAAAGFw//hUCCt8AAAADQj/+FQILGkAAACIZf/4VAgtbgAAAOQd//hUCC5nAAAAUJX/+FQIL2AAAAA87f/4VAgwPQAAAB3e//hUCDE6AAAAcab/+FQIMjMAAADFLv/4VAgzNAAAAKlW//hUCDQhAAAALDv/+FQINSYAAABAQ//4VAg2LwAAAPTL//hUCDcoAAAAmLP/+FQIOAUAAAB+FP/4VAg5AgAAABJs//hUCDoLAAAApuT/+FQIOwwAAADKnP/4VAg8GQAAAE/x//hUCD0eAAAAI4n/+FQIPhcAAACXAf/4VAg/EAAAAPt5//hUCEBqAAAAp0X/+FQIQW0AAADLPf/4VAhCZAAAAH+1//hUCENjAAAAE83/+FQIRHYAAACWoP/4VAhFcQAAAPrY//hUCEZ4AAAATlD/+FQIR38AAAAiKP/4VAhIUgAAAMSP//hUCElVAAAAqPf/+FQISlwAAAAcf//4VAhLWwAAAHAH//hUCExOAAAA9Wr/+FQITUkAAACZEv/4VAhOQAAAAC2a//hUCE9HAAAAQeL/+FQIUBoAAABg0f/4VAhRHQAAAAyp//hUCFIUAAAAuCH/+FQIUxMAAADUWf/4VAhUBgAAAFE0//hUCFUBAAAAPUz/+FQIVggAAACJxP/4VAhXDwAAAOW8//hUCFgiAAAAAxv/+FQIWSUAAABvY//4VAhaLAAAANvr//hUCFsrAAAAt5P/+FQIXD4AAAAy/v/4VAhdOQAAAF6G//hUCF4wAAAA6g7/+FQIXzcAAACGdv/4VAhgigAAAKho//hUCGGNAAAAxBD/+FQIYoQAAABwmP/4VAhjgwAAABzg//hUCGSWAAAAmY3/+FQIZZEAAAD19f/4VAhmmAAAAEF9//hUCGefAAAALQX/+HQIaAL/2AAAAL13";
export type FixtureTrack = { title: string; artist: string; album: string; albumArtist?: string };
export const fixtureTracks: FixtureTrack[] = [
  { title: "Song Alpha", artist: "Artist A", album: "Fixture Album", albumArtist: "Various" },
  { title: "Song Beta", artist: "Artist B", album: "Fixture Album", albumArtist: "Various" },
  { title: "Elsewhere", artist: "Artist C", album: "Other Album" },
];
function u32(value: number): number[] { return [value & 255, value >>> 8 & 255, value >>> 16 & 255, value >>> 24 & 255]; }
/** Replace FLAC metadata with STREAMINFO and deterministic Vorbis comments; retain audio frames. */
export function taggedFlac(audio: Uint8Array, track: FixtureTrack): Uint8Array {
  let end = 4;
  while (true) {
    const last = !!(audio[end] & 128);
    const length = audio[end + 1] * 65536 + audio[end + 2] * 256 + audio[end + 3];
    end += 4 + length;
    if (last) break;
  }
  const comments = [`TITLE=${track.title}`, `ARTIST=${track.artist}`, `ALBUM=${track.album}`,
    `ALBUMARTIST=${track.albumArtist ?? track.artist}`];
  const vendor = Buffer.from("Haiku M1-03 fixture");
  const body = [...u32(vendor.length), ...vendor, ...u32(comments.length)];
  for (const comment of comments) {
    const bytes = Buffer.from(comment);
    body.push(...u32(bytes.length), ...bytes);
  }
  const info = audio.slice(4, 42); info[0] = 0;
  const result = new Uint8Array(4 + info.length + 4 + body.length + audio.length - end);
  result.set(audio.slice(0, 4)); result.set(info, 4);
  result.set([132, body.length >>> 16 & 255, body.length >>> 8 & 255, body.length & 255], 42);
  result.set(body, 46); result.set(audio.slice(end), 46 + body.length);
  return result;
}
export async function prepareLibrary(root: string, recycledRows = 0, audio: Uint8Array = Buffer.from(silence, "base64")) {
  await mkdir(root, { recursive: true });
  const tracks = [...fixtureTracks, ...Array.from({ length: recycledRows }, (_, i) => ({
    title: `Zzz ${String(i).padStart(3, "0")}`, artist: "Virtual Artist", album: "Recycling",
  }))];
  for (let i = 0; i < tracks.length; i++) await writeFile(`${root}/${i}.flac`, taggedFlac(audio, tracks[i]));
  return tracks;
}
export async function removeLibrary(root: string) {
  for (const file of await readdir(root)) await unlink(`${root}/${file}`);
  await rmdir(root);
}
