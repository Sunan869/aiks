import {describe,it,expect} from "vitest";
import {WorkspaceController} from "./onboarding";
import type {UiPreferences} from "../../api/service";
const unseen:UiPreferences={onboarding:"unseen",selected_sources:[]};
function deferred<T>(){let resolve!:(value:T)=>void;let reject!:(error:Error)=>void;const promise=new Promise<T>((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};}
describe("preference read/write ordering",()=>{
  it("can retry reading after a superseded initial read and failed save",async()=>{
    const first=deferred<UiPreferences>();let reads=0;
    const c=new WorkspaceController({preferences:async()=>++reads===1?first.promise:unseen,finishOnboarding:async()=>{throw Error("unavailable");},saveSources:async()=>unseen});
    const loading=c.initialize();await c.finish(true);first.resolve(unseen);await loading;
    await c.initialize();expect(reads).toBe(2);expect(c.getSnapshot().preferences).toEqual(unseen);expect(c.getSnapshot().page).toBe("knowledge");
  });
  it("ignores a late read failure after successful user action",async()=>{
    const first=deferred<UiPreferences>();const skipped:UiPreferences={onboarding:"skipped",selected_sources:[]};
    const c=new WorkspaceController({preferences:()=>first.promise,finishOnboarding:async()=>skipped,saveSources:async()=>skipped});
    const loading=c.initialize();await c.finish(true);first.reject(Error("stale request"));await loading;
    expect(c.getSnapshot().error).toBe(null);expect(c.getSnapshot().preferences).toEqual(skipped);
  });
});
