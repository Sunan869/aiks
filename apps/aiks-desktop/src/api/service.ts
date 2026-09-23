/** Service business calls only. No URL, credentials or legacy business fallback. */
export type InvokeService = (command:string,args?:Record<string,unknown>)=>Promise<unknown>;
export interface Receipt {receipt_id:string;session_id:string;job_id:string;revision:number;state:string}
export interface Upload {id:string;state:string;source:string;attempt:number;error_code:string|null;receipt:Receipt|null}
export interface ServiceStatus {
  mode:"legacy"|"service_local";phase:string;error_code?:string|null;
  capabilities?:{instance_id:string;space_id:string;ai_assist:boolean;semantic_search:boolean}|null;
  providers?:{key:string;display_name:string;enabled:boolean}[];
}
export interface SearchHit {corpus:"session"|"knowledge";entity_id:string;title:string;snippet:string;revision:number}
export interface SearchResult {hits:SearchHit[];degraded:boolean;warnings:string[]}
export interface UiPreferences {onboarding:"unseen"|"skipped"|"completed";selected_sources:string[]}
export interface LocalSessionRow {key:string;title:string;project:string|null;updated_at:string|null;message_count:number;excluded:boolean}
export interface LocalSessionPage {items:LocalSessionRow[];total:number;discovered:number;complete:boolean;offset:number;limit:number;can_save:boolean}
export interface LocalPreview {title:string;messages:{role:string;text:string}[];truncated:boolean}
export interface ExclusionChange {excluded_count:number;paused:number;already_received:number}
export interface Job {job_id:string;status:string;revision:number;current_revision:number;error_code:string|null}
export function statusText(state:{receiptState?:string;jobState?:string}):string {
  switch(state.jobState){
    case "PENDING":return "已接收，等待处理";
    case "RUNNING":return "正在处理";
    case "DONE":return "处理完成";
    case "FAILED":return "处理失败";
    case "SUPERSEDED":return "已被新版本替代";
    default:return state.receiptState==="accepted"?"已接收，处理状态待查询":"等待发送";
  }
}
function object(value:unknown):Record<string,unknown>{
  if(!value||typeof value!=="object"||Array.isArray(value))throw new Error("invalid_service_response");
  return value as Record<string,unknown>;
}
function array(value:unknown):unknown[]{if(!Array.isArray(value))throw new Error("invalid_service_response");return value;}
export class ServiceApi {
  constructor(private readonly invoke:InvokeService){}
  private async call(command:string,args?:Record<string,unknown>):Promise<unknown>{
    try{return await this.invoke(command,args);}catch(error){throw new Error(typeof error==="string"?error:error instanceof Error?error.message:"service_unavailable");}
  }
  async status():Promise<ServiceStatus>{
    const value=object(await this.call("service_status"));
    if(!["legacy","service_local"].includes(String(value.mode))||typeof value.phase!=="string")throw new Error("invalid_service_response");
    return value as unknown as ServiceStatus;
  }
  async search(query:string,corpus?:"session"|"knowledge"):Promise<SearchResult>{
    const value=object(await this.call("service_search",corpus?{query,corpus}:{query}));
    const hits=array(value.hits);
    for(const raw of hits){const hit=object(raw);if(!["session","knowledge"].includes(String(hit.corpus))||typeof hit.entity_id!=="string"||typeof hit.title!=="string"||typeof hit.snippet!=="string"||typeof hit.revision!=="number")throw new Error("invalid_service_response");}
    if(typeof value.degraded!=="boolean"||!Array.isArray(value.warnings))throw new Error("invalid_service_response");
    return value as unknown as SearchResult;
  }
  async collect(sources:string[]):Promise<Record<string,unknown>>{
    return object(await this.call("service_collect_selected",{sources}));
  }
  async uploads():Promise<Upload[]>{return array(await this.call("service_uploads")) as Upload[];}
  async job(id:string):Promise<Job>{
    const value=object(await this.call("service_get_job",{id}));
    if(typeof value.status!=="string"||typeof value.job_id!=="string")throw new Error("invalid_service_response");
    return value as unknown as Job;
  }
  async receipt(id:string):Promise<Receipt>{
    const value=object(await this.call("service_get_receipt",{id}));
    if(value.state!=="accepted"||typeof value.revision!=="number")throw new Error("invalid_service_response");
    return value as unknown as Receipt;
  }
  async sessions(offset=0):Promise<Record<string,unknown>[]>{return array(object(await this.call("service_sessions",{offset})).items).map(object);}
  async knowledgeList(offset=0):Promise<Record<string,unknown>[]>{return array(object(await this.call("service_knowledge_list",{offset})).items).map(object);}
  async preferences():Promise<UiPreferences>{return preferences(await this.call("service_ui_preferences"));}
  async finishOnboarding(skipped:boolean):Promise<UiPreferences>{return preferences(await this.call("service_finish_onboarding",{skipped}));}
  async saveSources(sources:string[]):Promise<UiPreferences>{return preferences(await this.call("service_save_sources",{sources}));}
  async scanSessions(source:string):Promise<LocalSessionPage>{return sessionPage(await this.call("service_scan_sessions",{source}));}
  async browseSessions(source:string,query:string,offset:number):Promise<LocalSessionPage>{return sessionPage(await this.call("service_browse_sessions",{source,query,offset}));}
  async previewSession(source:string,key:string):Promise<LocalPreview>{
    const value=object(await this.call("service_preview_session",{source,key}));
    if(typeof value.title!=="string"||typeof value.truncated!=="boolean")throw new Error("invalid_session_preview");
    for(const raw of array(value.messages)){const message=object(raw);if(typeof message.role!=="string"||typeof message.text!=="string")throw new Error("invalid_session_preview");}
    return value as unknown as LocalPreview;
  }
  async excludeSessions(source:string,keys:string[],excluded:boolean):Promise<ExclusionChange>{
    const value=object(await this.call("service_exclude_sessions",{source,keys,excluded}));
    for(const key of ["excluded_count","paused","already_received"])if(!count(value[key]))throw new Error("invalid_selection_result");
    return value as unknown as ExclusionChange;
  }
  async detail(corpus:"session"|"knowledge",id:string):Promise<Record<string,unknown>>{
    return object(await this.call(corpus==="session"?"service_session":"service_knowledge",{id}));
  }
}
function count(value:unknown):value is number{return typeof value==="number"&&Number.isSafeInteger(value)&&value>=0;}
function preferences(raw:unknown):UiPreferences{
  const value=object(raw);
  if(!["unseen","skipped","completed"].includes(String(value.onboarding))||!Array.isArray(value.selected_sources)||value.selected_sources.some(s=>typeof s!=="string"))throw new Error("invalid_preferences");
  return value as unknown as UiPreferences;
}
function sessionPage(raw:unknown):LocalSessionPage{
  const value=object(raw);
  for(const key of ["total","discovered","offset","limit"])if(!count(value[key]))throw new Error("invalid_session_list");
  if(typeof value.complete!=="boolean"||typeof value.can_save!=="boolean"||Number(value.limit)===0)throw new Error("invalid_session_list");
  for(const raw of array(value.items)){
    const row=object(raw);
    if(typeof row.key!=="string"||typeof row.title!=="string"||!count(row.message_count)||typeof row.excluded!=="boolean"||!(row.project===null||typeof row.project==="string")||!(row.updated_at===null||typeof row.updated_at==="string"))throw new Error("invalid_session_list");
  }
  return value as unknown as LocalSessionPage;
}
export const serviceApi=new ServiceApi(async(command,args)=>{
  const {invoke}=await import("@tauri-apps/api/core");return invoke(command,args);
});
