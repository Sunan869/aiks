import {describe,expect,it,vi} from "vitest";
import {WorkspaceController} from "./onboarding";
import {ServiceApi,type UiPreferences} from "../../api/service";

function state(onboarding:UiPreferences["onboarding"]="unseen"):UiPreferences{return {onboarding,selected_sources:[]};}
function deferred<T>(){let resolve!:(value:T)=>void;const promise=new Promise<T>(r=>{resolve=r;});return {promise,resolve};}
describe("optional personal onboarding",()=>{
  it("offers setup once, persists skip and starts later sessions in knowledge",async()=>{
    let stored=state();const calls:string[]=[];
    const api=new ServiceApi(async(command,args)=>{
      calls.push(command);
      if(command==="service_ui_preferences")return stored;
      if(command==="service_finish_onboarding"){stored=state(args?.skipped?"skipped":"completed");return stored;}
      throw Error("unexpected side effect");
    });
    const first=new WorkspaceController(api);await first.initialize();
    expect(first.getSnapshot().page).toBe("setup");
    await first.finish(true);expect(first.getSnapshot().page).toBe("knowledge");
    const next=new WorkspaceController(api);await next.initialize();
    expect(next.getSnapshot().page).toBe("knowledge");
    expect(calls).toEqual(["service_ui_preferences","service_finish_onboarding","service_ui_preferences"]);
  });
  it("can leave the guide even if saving fails, without pretending persistence succeeded",async()=>{
    const api={preferences:vi.fn().mockResolvedValue(state()),finishOnboarding:vi.fn().mockRejectedValue(Error("disk full")),saveSources:vi.fn()};
    const c=new WorkspaceController(api);await c.initialize();await c.finish(true);
    expect(c.getSnapshot().page).toBe("knowledge");expect(c.getSnapshot().error).toContain("未保存");
    expect(c.getSnapshot().preferences?.onboarding).toBe("unseen");expect(api.saveSources).not.toHaveBeenCalled();
  });
  it("a late startup preferences reply cannot reopen setup or overwrite a saved decision",async()=>{
    const read=deferred<UiPreferences>();const c=new WorkspaceController({preferences:()=>read.promise,finishOnboarding:async()=>state("skipped"),saveSources:async()=>state("skipped")});
    const loading=c.initialize();await c.finish(true);read.resolve(state());await loading;
    expect(c.getSnapshot().page).toBe("knowledge");expect(c.getSnapshot().preferences?.onboarding).toBe("skipped");
  });
  it("clicking knowledge before preferences load does not later force a setup page",async()=>{
    const read=deferred<UiPreferences>();const c=new WorkspaceController({preferences:()=>read.promise,finishOnboarding:async()=>state("skipped"),saveSources:async()=>state("skipped")});
    const loading=c.initialize();c.navigate("knowledge");read.resolve(state());await loading;
    expect(c.getSnapshot().page).toBe("knowledge");
  });
  it("saving sources is not starting collection or enabling models",async()=>{
    const calls:string[]=[];const api=new ServiceApi(async(command,args)=>{
      calls.push(command);return {onboarding:"skipped",selected_sources:args?.sources??[]};
    });const c=new WorkspaceController(api);await c.saveSources(["workbuddy"]);
    expect(calls).toEqual(["service_save_sources"]);expect(c.getSnapshot().preferences?.selected_sources).toEqual(["workbuddy"]);
  });
});

describe("read-only browsing transport",()=>{
  it("only calls scan and preview actions, never submit or model routes",async()=>{
    const invoke=vi.fn().mockResolvedValueOnce({items:[],total:0,discovered:0,complete:true,offset:0,limit:30,can_save:false})
      .mockResolvedValueOnce({title:"本地预览",messages:[{role:"user",text:"hello"}],truncated:false});
    const api=new ServiceApi(invoke);await api.scanSessions("workbuddy");await api.previewSession("workbuddy","opaque-row");
    expect(invoke.mock.calls).toEqual([["service_scan_sessions",{source:"workbuddy"}],["service_preview_session",{source:"workbuddy",key:"opaque-row"}]]);
  });
  it("malformed browsing responses are errors rather than an empty list",async()=>{
    const api=new ServiceApi(async()=>({items:[],complete:true}));await expect(api.scanSessions("workbuddy")).rejects.toThrow();
  });
});
