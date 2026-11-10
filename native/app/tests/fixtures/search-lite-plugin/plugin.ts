import { definePlugin, field, Upstream } from "haiku";
export default definePlugin({
  id:"search.lite.fixture",name:"轻量曲库",version:"1.0.0",
  connection:{fields:{url:field.url("服务器")},identity:["url"],async verify(){return {};}},
  search:{async track({keyword},ctx){
    if(keyword==="来源错误")throw new Upstream("轻量来源暂时离线");
    return {items:[{id:"lite-1",title:`轻量 ${keyword}`,artists:[],duration:60}]};
  }},
  track:{async streams(id,ctx){return [{url:`${ctx.connection!.url}/song.flac`,format:"flac"}];}},
});
