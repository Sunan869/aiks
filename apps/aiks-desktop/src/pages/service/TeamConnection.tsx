import {useEffect,useState} from "react";
import {Cloud,ExternalLink,RefreshCw,ShieldCheck,UploadCloud} from "lucide-react";
import {serviceApi,type ServiceStatus} from "../../api/service";
import {button,primary,errorText} from "./shared";

interface TeamWorkspaceConfig {
  configured:boolean;
  web_url:string|null;
  base_url:string;
  knowledge_base_id:string;
  collector_url:string;
  remote_mode:boolean;
}

async function invokeNative<T>(command:string):Promise<T>{
  const {invoke}=await import("@tauri-apps/api/core");
  return invoke<T>(command);
}

export default function TeamConnection(){
  const [config,setConfig]=useState<TeamWorkspaceConfig|null>(null);
  const [status,setStatus]=useState<ServiceStatus|null>(null);
  const [error,setError]=useState<string|null>(null);
  const [opening,setOpening]=useState(false);

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
        <div className="mt-5 flex flex-wrap gap-3">
          <button className={primary} disabled={!config?.configured||opening} onClick={()=>void openWorkspace()}><ExternalLink size={15} className="mr-1 inline"/>{opening?"正在打开…":"打开 WeKnora 团队工作台"}</button>
        </div>
        {!config?.configured&&<p className="mt-4 rounded-lg bg-amber-50 p-3 text-sm text-amber-800">尚未配置 WeKnora Web 地址或私有知识库。请为远程采集配置 <code>[weknora].base_url</code>、<code>knowledge_base_id</code> 与 API Key 环境变量。</p>}
      </section>

      <div className="grid gap-4 md:grid-cols-3">
        <section className="rounded-xl border bg-white p-5"><UploadCloud size={20} className="text-blue-600"/><h2 className="mt-3 font-semibold">采集链路</h2><p className="mt-2 text-sm text-slate-600">{status?.phase==="ready"?"Collector 已连接":"Collector 尚未就绪"}</p><p className="mt-2 break-all text-xs text-slate-400">{config?.collector_url||"未配置 collector_url"}</p></section>
        <section className="rounded-xl border bg-white p-5"><ShieldCheck size={20} className="text-emerald-600"/><h2 className="mt-3 font-semibold">团队权限</h2><p className="mt-2 text-sm leading-6 text-slate-600">登录、用户直分享、组织分享和钉钉部门映射全部在 WeKnora 管理。</p></section>
        <section className="rounded-xl border bg-white p-5"><Cloud size={20} className="text-violet-600"/><h2 className="mt-3 font-semibold">同步队列</h2><p className="mt-2 text-sm text-slate-600">待同步 {pending} · 终止失败 {terminal}</p><p className="mt-2 text-xs text-slate-400">网络恢复后由 collector durable outbox 自动重试。</p></section>
      </div>

      <section className="rounded-xl border bg-white p-5">
        <h2 className="font-semibold">当前目标</h2>
        <dl className="mt-3 grid gap-3 text-sm md:grid-cols-2">
          <div><dt className="text-slate-400">WeKnora</dt><dd className="mt-1 break-all">{config?.base_url||"未配置"}</dd></div>
          <div><dt className="text-slate-400">私有 AIKS Knowledge Base</dt><dd className="mt-1 break-all">{config?.knowledge_base_id||"未配置"}</dd></div>
        </dl>
        <p className="mt-4 text-xs leading-6 text-slate-500">员工 Session 先进入自己的私有 WeKnora KB，再由 WeKnora 的用户/组织/部门权限决定谁可以看到；Desktop 不会把个人历史直接发布到公司公共库。</p>
      </section>
    </div>
  </div>;
}
