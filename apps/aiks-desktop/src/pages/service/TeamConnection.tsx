import {useEffect,useState} from "react";
import {Cloud,ExternalLink,LogIn,RefreshCw,ShieldCheck,UploadCloud} from "lucide-react";
import {serviceApi,type ServiceStatus} from "../../api/service";
import {button,primary,errorText} from "./shared";

interface TeamWorkspaceConfig {
  configured:boolean;
  web_url:string|null;
  base_url:string;
  knowledge_base_id:string;
  collector_url:string;
  remote_mode:boolean;
  login_required:boolean;
  allow_insecure_http:boolean;
}
interface TeamLoginResult {
  state:"pending"|"connected"|"browser_opened";
  knowledge_base?:string;
}

async function invokeNative<T>(command:string):Promise<T>{
  const {invoke}=await import("@tauri-apps/api/core");
  return invoke<T>(command);
}

export default function TeamConnection(){
  const [config,setConfig]=useState<TeamWorkspaceConfig|null>(null);
  const [status,setStatus]=useState<ServiceStatus|null>(null);
  const [error,setError]=useState<string|null>(null);
  const [message,setMessage]=useState<string|null>(null);
  const [opening,setOpening]=useState(false);
  const [loginPending,setLoginPending]=useState(false);

  async function refresh(){
    setError(null);
    try{
      const [workspace,current]=await Promise.all([
        invokeNative<TeamWorkspaceConfig>("service_team_workspace"),
        serviceApi.status(),
      ]);
      setConfig(workspace);setStatus(current);
    }catch(e){setError(errorText(e));}
  }
  useEffect(()=>{void refresh();},[]);

  useEffect(()=>{
    if(!loginPending)return;
    let cancelled=false;
    let timer:number|undefined;
    const poll=async()=>{
      try{
        const result=await invokeNative<TeamLoginResult>("service_finish_team_login");
        if(cancelled)return;
        if(result.state==="connected"){
          setLoginPending(false);
          setMessage(`登录完成，${result.knowledge_base??"AIKS Sessions"} 已自动准备并连接。`);
          await refresh();
          return;
        }
      }catch(e){
        if(cancelled)return;
        setLoginPending(false);
        setError(errorText(e));
        return;
      }
      if(!cancelled)timer=window.setTimeout(()=>void poll(),1200);
    };
    timer=window.setTimeout(()=>void poll(),600);
    return()=>{cancelled=true;if(timer!==undefined)window.clearTimeout(timer);};
  },[loginPending]);

  async function beginLogin(){
    setError(null);setMessage(null);
    try{
      await invokeNative<TeamLoginResult>("service_begin_team_login");
      setLoginPending(true);
      setMessage("已在系统浏览器打开 WeKnora 登录。授权完成后会自动创建/复用 AIKS Sessions 并连接，无需再填写 KB ID 或 API Key。");
    }catch(e){setError(errorText(e));}
  }

  async function openWorkspace(){
    setOpening(true);setError(null);
    try{await invokeNative("service_open_team_workspace");}
    catch(e){setError(errorText(e));}
    finally{setOpening(false);}
  }

  const pending=Number((status as any)?.weknora?.pending??0);
  const terminal=Number((status as any)?.weknora?.terminal??0);
  return <div className="h-full overflow-auto bg-slate-50 p-6">
    <div className="mx-auto max-w-5xl space-y-5">
      <section className="rounded-2xl border bg-white p-6">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div><h1 className="flex items-center gap-2 text-2xl font-semibold"><Cloud size={22}/>WeKnora 团队工作台</h1>
            <p className="mt-2 max-w-3xl text-sm leading-7 text-slate-600">团队知识、钉钉登录、部门权限、分享、检索与 RAG 已统一由 WeKnora 提供。AIKS Desktop 只负责本机 Session 采集、脱敏和可靠同步，不再维护第二套团队账号或 ACL。</p>
          </div>
          <button className={button} onClick={()=>void refresh()}><RefreshCw size={15} className="mr-1 inline"/>刷新状态</button>
        </div>
        {error&&<p role="alert" className="mt-4 rounded-lg bg-red-50 p-3 text-sm text-red-700">{error}</p>}
        {message&&<p className="mt-4 rounded-lg bg-blue-50 p-3 text-sm text-blue-700">{message}</p>}
        {config?.allow_insecure_http&&<p className="mt-4 rounded-lg bg-amber-50 p-3 text-sm text-amber-800">当前已显式开启 <code>backend.allow_insecure_http=true</code>，仅建议用于联调；公网生产环境请恢复 HTTPS。</p>}
        <div className="mt-5 flex flex-wrap gap-3">
          {config?.remote_mode&&config.login_required
            ?<button className={primary} disabled={loginPending} onClick={()=>void beginLogin()}><LogIn size={15} className="mr-1 inline"/>{loginPending?"等待浏览器登录…":"登录 WeKnora 并自动初始化"}</button>
            :<button className={primary} disabled={!config?.configured||opening} onClick={()=>void openWorkspace()}><ExternalLink size={15} className="mr-1 inline"/>{opening?"正在打开…":"打开 WeKnora 团队工作台"}</button>}
        </div>
        {config?.remote_mode&&config.login_required&&<p className="mt-4 rounded-lg bg-slate-100 p-3 text-sm text-slate-700">首次连接只需要完成 WeKnora/钉钉登录。服务端会自动创建或复用 <strong>AIKS Sessions</strong>，生成仅限该 KB 的 scoped API Key，并把凭据直接交给 Desktop 原生进程。</p>}
        {!config?.configured&&!config?.login_required&&<p className="mt-4 rounded-lg bg-amber-50 p-3 text-sm text-amber-800">请先配置 WeKnora 地址与远程 Collector 地址；KB ID 和用户 API Key 不再需要手工配置。</p>}
      </section>

      <div className="grid gap-4 md:grid-cols-3">
        <section className="rounded-xl border bg-white p-5"><UploadCloud size={20} className="text-blue-600"/><h2 className="mt-3 font-semibold">采集链路</h2><p className="mt-2 text-sm text-slate-600">{status?.phase==="ready"?"Collector 已连接":status?.phase==="waiting_for_login"?"等待团队登录":"Collector 尚未就绪"}</p><p className="mt-2 break-all text-xs text-slate-400">{config?.collector_url||"未配置 collector_url"}</p></section>
        <section className="rounded-xl border bg-white p-5"><ShieldCheck size={20} className="text-emerald-600"/><h2 className="mt-3 font-semibold">团队权限</h2><p className="mt-2 text-sm leading-6 text-slate-600">登录、用户直分享、组织分享和钉钉部门映射全部在 WeKnora 管理。</p></section>
        <section className="rounded-xl border bg-white p-5"><Cloud size={20} className="text-violet-600"/><h2 className="mt-3 font-semibold">同步队列</h2><p className="mt-2 text-sm text-slate-600">待同步 {pending} · 终止失败 {terminal}</p><p className="mt-2 text-xs text-slate-400">网络恢复后由 collector durable outbox 自动重试。</p></section>
      </div>

      <section className="rounded-xl border bg-white p-5">
        <h2 className="font-semibold">当前目标</h2>
        <dl className="mt-3 grid gap-3 text-sm md:grid-cols-2">
          <div><dt className="text-slate-400">WeKnora</dt><dd className="mt-1 break-all">{config?.base_url||"未配置"}</dd></div>
          <div><dt className="text-slate-400">私有 AIKS Knowledge Base</dt><dd className="mt-1 break-all">{config?.knowledge_base_id||"登录后自动准备"}</dd></div>
        </dl>
        <p className="mt-4 text-xs leading-6 text-slate-500">员工 Session 先进入自己的私有 WeKnora KB，再由 WeKnora 的用户/组织/部门权限决定谁可以看到；Desktop 不会把个人历史直接发布到公司公共库。</p>
      </section>
    </div>
  </div>;
}
