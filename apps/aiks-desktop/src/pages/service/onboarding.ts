import type {ServiceApi, UiPreferences} from "../../api/service";
export type WorkspacePage = "knowledge" | "sessions" | "sync" | "models" | "tasks" | "setup";
export interface WorkspaceState {page:WorkspacePage; preferences:UiPreferences|null; error:string|null}
/** Navigation never waits for models, collection, uploads or a successful save. */
export class WorkspaceController {
  private state:WorkspaceState={page:"knowledge",preferences:null,error:null};
  private listeners=new Set<()=>void>();
  private touched=false;
  private revision=0;
  private loading:Promise<void>|null=null;
  constructor(private api:Pick<ServiceApi,"preferences"|"finishOnboarding"|"saveSources">){}
  getSnapshot=()=>this.state;
  subscribe=(listener:()=>void)=>{this.listeners.add(listener);return()=>{this.listeners.delete(listener);};};
  private update(change:Partial<WorkspaceState>){this.state={...this.state,...change};this.listeners.forEach(fn=>fn());}
  initialize(){
    if(this.loading)return this.loading;
    const revision=this.revision;
    this.loading=this.api.preferences().then(preferences=>{
      if(revision!==this.revision)return;
      this.update({preferences,page:!this.touched&&preferences.onboarding==="unseen"?"setup":this.state.page,error:null});
    }).catch(()=>{this.update({error:"暂时无法读取首次设置记录，仍可进入知识库。"});this.loading=null;});
    return this.loading;
  }
  navigate(page:WorkspacePage){
    this.touched=true;
    const dismiss=this.state.page==="setup"&&page!=="setup";
    this.update({page});
    if(dismiss)void this.persistFinish(true);
  }
  async finish(skipped:boolean){this.touched=true;this.update({page:"knowledge"});await this.persistFinish(skipped);}
  private async persistFinish(skipped:boolean){
    this.revision++;
    try{this.update({preferences:await this.api.finishOnboarding(skipped),error:null});}
    catch{this.update({error:"已进入知识库，但跳过状态未保存；下次启动可能再次显示引导。"});}
  }
  async saveSources(sources:string[]){
    this.revision++;
    const preferences=await this.api.saveSources(sources);
    this.update({preferences});
  }
}
