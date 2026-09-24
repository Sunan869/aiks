import {describe,expect,it,vi} from "vitest";
import {renderToStaticMarkup} from "react-dom/server";
import {TeamApi,TeamRequestGate,buildImportRequest} from "../../api/team";
import TeamConnection from "./TeamConnection";
import PublishToTeam from "./PublishToTeam";

describe("team sharing desktop boundary",()=>{
  it("keeps team access explicit and never auto-uploads personal history",()=>{
    const html=renderToStaticMarkup(<TeamConnection/>);
    expect(html).toContain("团队空间");expect(html).toContain("添加公司连接");expect(html).toContain("不会迁移或自动上传你的个人历史");
    expect(html).not.toContain("access_token");expect(html).not.toContain("refresh_token");
  });
  it("drops late results after switching connections",()=>{
    const gate=new TeamRequestGate();gate.select("connection-a");const old=gate.begin()!;gate.select("connection-b");
    expect(gate.accepts(old)).toBe(false);const current=gate.begin()!;expect(gate.accepts(current)).toBe(true);
  });
  it("uses fixed native commands and read-only share grants",async()=>{
    const invoke=vi.fn(async(command:string)=>command==="team_connection_status"?[]:command==="team_replace_shares"?{grant_version:4}:{});
    const api=new TeamApi(invoke);await api.connections();
    const version=await api.replaceShares("c","k",3,[{target_type:"org",target_id:"research",include_descendants:false,permission:"read"}]);
    expect(version).toBe(4);
    expect(invoke).toHaveBeenLastCalledWith("team_replace_shares",{connectionId:"c",knowledgeId:"k",expectedGrantVersion:3,grants:[{target_type:"org",target_id:"research",include_descendants:false,permission:"read"}]});
  });
  it("builds a private-copy import without source session or identity fields",()=>{
    const request=buildImportRequest({id:"local-1",title:"Local knowledge",content:"body",content_revision:7,session:{secret:"do-not-send"}});
    expect(request).toEqual({operationId:"import_local-1_7",title:"Local knowledge",markdown:"body",sourceFingerprint:"personal:local-1:revision:7"});
    expect(JSON.stringify(request)).not.toContain("session");expect(JSON.stringify(request)).not.toContain("owner");expect(JSON.stringify(request)).not.toContain("company");
  });
  it("renders publish as an explicit confirmation action",()=>{
    const html=renderToStaticMarkup(<PublishToTeam detail={{id:"k1",title:"One",content:"Body",content_revision:1}}/>);
    expect(html).toContain("发布到公司");expect(html).not.toContain("确认发布");
  });
});
