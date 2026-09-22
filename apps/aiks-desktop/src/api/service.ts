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
  async search(query:string):Promise<SearchResult>{
    const value=object(await this.call("service_search",{query}));
    const hits=array(value.hits);
    for(const raw of hits){const hit=object(raw);if(!["session","knowledge"].includes(String(hit.corpus))||typeof hit.entity_id!=="string"||typeof hit.title!=="string"||typeof hit.snippet!=="string"||typeof hit.revision!=="number")throw new Error("invalid_service_response");}
    if(typeof value.degraded!=="boolean"||!Array.isArray(value.warnings))throw new Error("invalid_service_response");
    return value as unknown as SearchResult;
  }
  async collect(sources:string[],excludeIds:string[]):Promise<Record<string,unknown>>{
    return object(await this.call("service_collect_selected",{sources,excludeIds}));
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
  async sessions():Promise<Record<string,unknown>[]>{return array(object(await this.call("service_sessions")).items).map(object);}
  async knowledgeList():Promise<Record<string,unknown>[]>{return array(object(await this.call("service_knowledge_list")).items).map(object);}
  async detail(corpus:"session"|"knowledge",id:string):Promise<Record<string,unknown>>{
    return object(await this.call(corpus==="session"?"service_session":"service_knowledge",{id}));
  }
}
export const serviceApi=new ServiceApi(async(command,args)=>{
  const {invoke}=await import("@tauri-apps/api/core");return invoke(command,args);
});
