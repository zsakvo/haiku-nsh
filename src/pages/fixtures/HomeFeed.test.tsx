import { expect, open, test } from "nsh/test";
const options={platform:"macos" as const,width:1100,height:700,network:"live" as const,clock:{}};

test("mixed home feed fills the viewport and continues beyond empty and repeated-template pages",async()=>{
  const app=await open("../../../HomeFeedFixture.tsx",options);
  await app.waitFor("role:button[歌单 1]");
  await app.waitFor("role:button[播放歌曲 1]");
  await app.press("role:button[播放歌曲 1]");
  await app.waitFor("text:播放 歌曲 1");
  // Small first pages must load without a wheel event; page 3 contains no supported cards.
  await app.waitFor("role:button[歌单 4]");
  for(let i=0;i<12&&!app.shows("role:button[歌单 12]");i++){
    await app.scroll("id:home-scroll",600);
    await app.clock!.runFor(120);
  }
  await app.waitFor("role:button[歌单 12]");
  expect(app.shows("role:button[加载更多推荐]")).toBe(false);
  await app.setViewport(700,700);
  await app.press("深色");
  await app.scroll("id:home-scroll",10000);
  await app.waitFor("role:button[歌单 12]");
});

test("continuation errors retry in place without resetting loaded recommendations",async()=>{
  const app=await open("../../../HomeFeedFixture.tsx",{...options,height:400});
  await app.waitFor("role:button[歌单 1]");
  await app.press("下次续页失败");
  for(let i=0;i<8&&!app.shows("text:Error: 续页暂时离线");i++){
    await app.scroll("id:home-scroll",500);
    await app.clock!.runFor(120);
  }
  await app.waitFor("text:Error: 续页暂时离线");
  await app.press("role:button[重试]");
  await app.waitUntil(()=>!app.shows("text:Error: 续页暂时离线"));
  if(!app.shows("role:button[歌单 1]"))await app.scroll("id:home-scroll",-100000);
  await app.waitFor("role:button[歌单 1]");
});

test("refresh discards buffered pages from the previous feed generation",async()=>{
  const app=await open("../../../HomeFeedFixture.tsx",options);
  await app.waitFor("role:button[歌单 1]");
  await app.press("延迟续页");
  await app.press("刷新推荐夹具");
  await app.waitFor("role:button[歌单 1 · 刷新2]");
  await app.clock!.runFor(2000);
  await app.waitFor("role:button[歌单 2 · 刷新2]");
  expect(app.shows("role:button[歌单 2]")).toBe(false);
});
