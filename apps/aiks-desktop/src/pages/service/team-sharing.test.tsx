import {describe,expect,it} from "vitest";
import {renderToStaticMarkup} from "react-dom/server";
import TeamConnection from "./TeamConnection";

describe("WeKnora team desktop boundary",()=>{
  it("delegates team knowledge and ACLs to WeKnora",()=>{
    const html=renderToStaticMarkup(<TeamConnection/>);
    expect(html).toContain("WeKnora 团队工作台");
    expect(html).toContain("AIKS Desktop 只负责本机 Session 采集");
    expect(html).toContain("登录、用户直分享、组织分享和钉钉部门映射全部在 WeKnora 管理");
    expect(html).not.toContain("添加公司连接");
    expect(html).not.toContain("完成登录");
    expect(html).not.toContain("管理分享");
    expect(html).not.toContain("access_token");
    expect(html).not.toContain("refresh_token");
  });
});
