// Development acceptance fixture only; no player interface. Remove in M1-03.
import "./App";
import { Core } from "haiku/native";

async function verify() {
  const core = await Core.devPlayFiles([
    ".artifacts/m0-05/haiku-test-voice.m4a",
  ]);
  await core.devVerifyPlayback();
  console.log("M1-02 real playback acceptance passed");
}
void verify().catch((error) => console.error("M1-02 real playback acceptance failed", error));
