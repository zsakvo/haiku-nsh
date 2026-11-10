import { expect,open,test } from "nsh/test";
const options={platform:"macos" as const,width:1200,height:800,network:"live" as const};
async function replace(app:Awaited<ReturnType<typeof open>>,selector:string,text:string) {
  await app.clear(selector);
  if(text)await app.type(selector,text);
}
test("search playback, types/sources, stale queries, pagination, errors and history",async()=>{
  const app=await open("../../../SearchFixture.tsx",options);
  await app.waitFor("text:搜索历史");
  await app.waitFor("id:search-tabs >> role:button[曲目]");
  await replace(app,"label:搜索词","潮汐");await app.press("id:page-search >> role:button[搜索]");
  await app.waitFor("text:已显示 5 项");
  await app.press("label:潮汐 · 2");await app.press("label:潮汐 · 2");
  await app.waitFor("text:队列 5 首 · 潮汐 · 2");await app.press("label:播放 / 暂停");
  await app.press("加载更多结果");await app.waitFor("text:已显示 7 项");
  await app.press("id:search-tabs >> role:button[专辑]");await app.waitFor("role:button[潮汐 · 专辑]");
  await replace(app,"label:搜索词","慢请求");await app.wait(100);
  await replace(app,"label:搜索词","新查询");await app.waitFor("role:button[新查询 · 专辑]");
  await app.wait(1100);expect(app.shows("role:button[慢请求 · 专辑]")).toBe(false);
  await replace(app,"label:搜索词","慢请求");await app.wait(100);
  await app.press("id:search-tabs >> role:button[曲目]");await replace(app,"label:搜索词","新查询");await app.waitFor("label:新查询 · 1");
  await app.wait(1100);expect(app.shows("label:慢请求 · 1")).toBe(false);
  await replace(app,"label:搜索词","慢请求");await app.wait(100);
  await app.press("label:来源");await app.press("role:option[轻量曲库]");
  await app.waitFor("label:轻量 慢请求");await app.wait(1100);
  expect(app.shows("label:慢请求 · 1")).toBe(false);
  expect(app.shows("id:search-tabs >> role:button[专辑]")).toBe(false);
  await replace(app,"label:搜索词","来源错误");await app.waitFor("text:Error: UPSTREAM_ERROR: 轻量来源暂时离线");
  await replace(app,"label:搜索词","");await app.waitFor("text:搜索历史");
  await app.waitFor("id:search-tabs >> role:button[曲目]");
  expect(app.shows("id:search-history >> role:button[潮汐]")).toBe(true);
  await app.press("label:删除搜索 潮汐");await app.waitUntil(()=>!app.shows("id:search-history >> role:button[潮汐]"));
  await app.press("清空搜索历史…");await app.press("role:button[清空]");await app.waitFor("text:还没有搜索记录");
});

test("command debounce, full first-page queue, stale input, view all and Escape",async()=>{
  const app=await open("../../../SearchFixture.tsx",options);
  await app.waitFor("text:搜索历史");
  await app.waitFor("id:search-tabs >> role:button[曲目]");
  await app.keyboard.press("mod+k");await replace(app,"label:查找命令","防抖");
  await app.wait(120);
  expect(app.shows("text:防抖 · 1 · 林间回声")).toBe(false);
  await replace(app,"label:查找命令","潮汐");await app.wait(300);
  await app.waitFor("text:潮汐 · 2 · 林间回声");
  expect(app.shows("text:潮汐 · 4 · 林间回声")).toBe(false);
  await app.press("text:潮汐 · 2 · 林间回声");await app.waitFor("text:队列 5 首 · 潮汐 · 2");
  await app.press("label:播放 / 暂停");
  await app.press("读取搜索调用");await app.waitUntil(()=>app.texts().some(t=>t.startsWith("搜索调用 [")));
  expect(app.texts().find(t=>t.startsWith("搜索调用 ["))!.includes("防抖")).toBe(false);
  await app.keyboard.press("mod+k");await replace(app,"label:查找命令","慢请求");await app.wait(300);await app.wait(100);
  await replace(app,"label:查找命令","新面板");await app.wait(300);
  await app.waitFor("text:新面板 · 1 · 林间回声");await app.wait(1100);
  expect(app.shows("text:慢请求 · 1 · 林间回声")).toBe(false);
  await app.press("text:在搜索页查看全部");await app.waitFor("label:新面板 · 1");
  expect(app.value("label:搜索词")).toBe("新面板");
  await app.keyboard.press("mod+k");await replace(app,"label:查找命令","关闭测试");await app.keyboard.press("escape");
  await app.waitUntil(()=>!app.shows("label:查找命令"));
});
