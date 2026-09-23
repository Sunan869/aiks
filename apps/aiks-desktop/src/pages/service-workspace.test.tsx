import {describe,expect,it} from "vitest";
import {renderToStaticMarkup} from "react-dom/server";
import ServiceStatusPage from "./ServiceStatusPage";

// The shell must be available even before the native Service answers.
describe("personal Service workspace navigation",()=>{
  it("keeps knowledge and conversation navigation visible before setup or service readiness",()=>{
    const html=renderToStaticMarkup(<ServiceStatusPage/>);
    expect(html).toContain('aria-label="主导航"');
    expect(html).toContain("知识库");
    expect(html).toContain("会话记录");
    expect(html).toContain("采集与同步");
  });
  it("never asks people to type upstream session identifiers",()=>{
    const html=renderToStaticMarkup(<ServiceStatusPage/>);
    expect(html).not.toContain("排除会话 ID");
    expect(html).not.toContain("每行填写一个来源会话 ID");
    expect(html).not.toContain("JSON.stringify");
  });
});
