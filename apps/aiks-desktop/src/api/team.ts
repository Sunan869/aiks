/** Native-only team bridge. Tokens remain in Rust and never enter WebView state. */
export type InvokeTeam = (command:string,args?:Record<string,unknown>)=>Promise<unknown>;

export type TeamConnectionState="signed_out"|"signed_in"|"credentials_missing";
export interface TeamConnectionStatus {
  connection_id:string;origin:string;state:TeamConnectionState;
  instance_id:string|null;company_id:string|null;user_id:string|null;space_id:string|null;display_name:string|null;
}
export type ShareSource="mine"|"shared_to_me"|"department";
export interface TeamKnowledgeRow {
  id:string;title:string;revision:number|null;current_revision:number;content_revision:number|null;
  can_manage:boolean;share_source:ShareSource;stale:boolean;content_state:string;
}
export interface TeamKnowledgeDetail extends TeamKnowledgeRow, Record<string,unknown> {
  summary:string;category:string;tags:string[];content:string|null;
}
export interface DirectoryEntry {target_type:"user"|"org";target_id:string;display_name:string}
export interface ShareGrant {target_type:"user"|"org";target_id:string;include_descendants:boolean;permission:"read"}
export interface ShareState {grant_version:number;grants:ShareGrant[]}
export interface ImportReceipt {operation_id:string;knowledge_id:string;content_revision:number}
export interface ImportRequest {operationId:string;title:string;markdown:string;sourceFingerprint:string}

function object(value:unknown):Record<string,unknown>{if(!value||typeof value!=="object"||Array.isArray(value))throw new Error("invalid_team_response");return value as Record<string,unknown>;}
function array(value:unknown):unknown[]{if(!Array.isArray(value))throw new Error("invalid_team_response");return value;}
function text(value:unknown):value is string{return typeof value==="string"&&value.length>0;}
function count(value:unknown):value is number{return typeof value==="number"&&Number.isSafeInteger(value)&&value>=0;}
function nullableText(value:unknown):value is string|null{return value===null||typeof value==="string";}
function connection(raw:unknown):TeamConnectionStatus{
  const value=object(raw);
  if(!text(value.connection_id)||!text(value.origin)||!["signed_out","signed_in","credentials_missing"].includes(String(value.state)))throw new Error("invalid_team_response");
  for(const key of ["instance_id","company_id","user_id","space_id","display_name"])if(!nullableText(value[key]))throw new Error("invalid_team_response");
  return value as unknown as TeamConnectionStatus;
}
function shareSource(value:unknown):value is ShareSource{return value==="mine"||value==="shared_to_me"||value==="department";}
function knowledgeRow(raw:unknown):TeamKnowledgeRow{
  const value=object(raw);
  if(!text(value.id)||typeof value.title!=="string"||!(value.revision===null||count(value.revision))||!count(value.current_revision)||!(value.content_revision===null||count(value.content_revision))||typeof value.can_manage!=="boolean"||!shareSource(value.share_source)||typeof value.stale!=="boolean"||typeof value.content_state!=="string")throw new Error("invalid_team_response");
  return value as unknown as TeamKnowledgeRow;
}
function grant(raw:unknown):ShareGrant{
  const value=object(raw);
  if(!["user","org"].includes(String(value.target_type))||!text(value.target_id)||typeof value.include_descendants!=="boolean"||value.permission!=="read")throw new Error("invalid_team_response");
  if(value.target_type==="user"&&value.include_descendants)throw new Error("invalid_team_response");
  return value as unknown as ShareGrant;
}

export class TeamApi {
  constructor(private readonly invoke:InvokeTeam){}
  private async call(command:string,args?:Record<string,unknown>):Promise<unknown>{try{return await this.invoke(command,args);}catch(error){throw new Error(typeof error==="string"?error:error instanceof Error?error.message:"team_unavailable");}}
  async connections():Promise<TeamConnectionStatus[]>{return array(await this.call("team_connection_status")).map(connection);}
  async addConnection(origin:string):Promise<TeamConnectionStatus>{return connection(await this.call("team_add_connection",{origin}));}
  async beginLogin(connectionId:string):Promise<void>{object(await this.call("team_begin_login",{connectionId}));}
  async finishLogin(connectionId:string):Promise<TeamConnectionStatus>{return connection(await this.call("team_finish_login",{connectionId}));}
  async logout(connectionId:string):Promise<void>{await this.call("team_logout",{connectionId});}
  async knowledgeList(connectionId:string,offset=0):Promise<TeamKnowledgeRow[]>{
    const value=object(await this.call("team_knowledge_list",{connectionId,offset}));
    return array(value.items).map(knowledgeRow);
  }
  async knowledge(connectionId:string,id:string):Promise<TeamKnowledgeDetail>{
    const value=object(await this.call("team_knowledge",{connectionId,id}));const row=knowledgeRow(value);
    if(typeof value.summary!=="string"||typeof value.category!=="string"||!Array.isArray(value.tags)||value.tags.some(v=>typeof v!=="string")||!(value.content===null||typeof value.content==="string"))throw new Error("invalid_team_response");
    return {...row,summary:value.summary,category:value.category,tags:value.tags as string[],content:value.content as string|null};
  }
  async directorySearch(connectionId:string,query:string):Promise<DirectoryEntry[]>{
    const value=object(await this.call("team_directory_search",{connectionId,query}));
    return array(value.items).map(raw=>{const item=object(raw);if(!["user","org"].includes(String(item.target_type))||!text(item.target_id)||typeof item.display_name!=="string")throw new Error("invalid_team_response");return item as unknown as DirectoryEntry;});
  }
  async shares(connectionId:string,knowledgeId:string):Promise<ShareState>{
    const value=object(await this.call("team_get_shares",{connectionId,knowledgeId}));
    if(!count(value.grant_version))throw new Error("invalid_team_response");
    return {grant_version:value.grant_version,grants:array(value.grants).map(grant)};
  }
  async replaceShares(connectionId:string,knowledgeId:string,expectedGrantVersion:number,grants:ShareGrant[]):Promise<number>{
    if(!count(expectedGrantVersion)||grants.some(v=>v.permission!=="read"))throw new Error("invalid_input");
    const value=object(await this.call("team_replace_shares",{connectionId,knowledgeId,expectedGrantVersion,grants}));
    if(!count(value.grant_version))throw new Error("invalid_team_response");return value.grant_version;
  }
  async importKnowledge(connectionId:string,input:ImportRequest):Promise<ImportReceipt>{
    const value=object(await this.call("team_import_knowledge",{connectionId,operationId:input.operationId,title:input.title,markdown:input.markdown,sourceFingerprint:input.sourceFingerprint}));
    if(!text(value.operation_id)||!text(value.knowledge_id)||!count(value.content_revision))throw new Error("invalid_team_response");
    return value as unknown as ImportReceipt;
  }
}
export interface RequestTicket {connectionId:string;generation:number}
export class TeamRequestGate {
  private generation=0;private connectionId:string|null=null;
  select(connectionId:string|null){this.connectionId=connectionId;this.generation++;}
  begin():RequestTicket|null{return this.connectionId?{connectionId:this.connectionId,generation:this.generation}:null;}
  accepts(ticket:RequestTicket){return ticket.connectionId===this.connectionId&&ticket.generation===this.generation;}
  current(){return this.connectionId;}
}
export function buildImportRequest(detail:Record<string,unknown>):ImportRequest{
  const id=String(detail.id??"");const title=String(detail.title??"").trim();const markdown=typeof detail.content==="string"?detail.content:"";
  if(!id||!title||!markdown)throw new Error("content_unavailable");
  const revision=String(detail.content_revision??detail.current_revision??detail.revision??0);
  const safe=id.replace(/[^A-Za-z0-9_-]/g,"_").slice(0,80);
  return {operationId:`import_${safe}_${revision}`.slice(0,128),title,markdown,sourceFingerprint:`personal:${id}:revision:${revision}`.slice(0,512)};
}
export const teamApi=new TeamApi(async(command,args)=>{const {invoke}=await import("@tauri-apps/api/core");return invoke(command,args);});
