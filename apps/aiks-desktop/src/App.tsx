import {useEffect,useState} from "react";
import LegacyApp from "./LegacyApp";
import {ProviderCatalogProvider} from "./ProviderCatalog";
import {isTauriContext,shouldUseMock} from "./api/index";
import {serviceApi} from "./api/service";
import ServiceStatusPage from "./pages/ServiceStatusPage";

export default function App(){
  const [mode,setMode]=useState<string|null>(shouldUseMock()?"legacy":null);
  const [error,setError]=useState<string|null>(null);
  useEffect(()=>{
    if(shouldUseMock())return;
    if(!isTauriContext()){setError("请通过 AIKS 桌面程序启动；浏览器页面不会连接本地资料或生成模拟结果。");return;}
    let cancelled=false;
    const load=()=>serviceApi.status().then(status=>{if(!cancelled){setMode(status.mode);setError(null);}}).catch(()=>{if(!cancelled)setError("无法确定后端模式，请检查终端启动错误与 backend.mode 配置。");});
    void load();const timer=setInterval(()=>{if(mode===null)void load();},2000);
    return()=>{cancelled=true;clearInterval(timer);};
  },[mode]);
  if(mode==="service_local")return <ServiceStatusPage/>;
  if(mode==="legacy")return <ProviderCatalogProvider><LegacyApp/></ProviderCatalogProvider>;
  return <main className="p-10 text-gray-700"><h1 className="text-xl font-semibold">AIKS</h1><p className="mt-4">{error??"正在连接本机知识服务…"}</p></main>;
}
