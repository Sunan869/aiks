import { describe, expect, it } from "vitest";
import { MockAiksApi } from "./mock";
import organizationPage from "../pages/KnowledgeDetailPage.tsx?raw";
import coreScope from "../../../../crates/aiks-core/src/knowledge/organization_scope.rs?raw";
import rustCommand from "../../src-tauri/src/knowledge_commands.rs?raw";

describe("S3.4 authoritative source scope before AI and reviewed writes", () => {
  it("validates canonical Session project instead of trusting matching display names", async () => {
    const api = new MockAiksApi();
    expect(await api.validateOrganizationScope(["kn_0001"])).toBe(true);
    expect(await api.validateOrganizationScope(["kn_0001", "kn_0005"])).toBe(true);
    await expect(api.validateOrganizationScope(["kn_0001", "kn_0002"])).rejects.toThrow();
    await expect(api.validateOrganizationScope(["kn_0001", "kn_0004"])).rejects.toThrow();
    await expect(api.validateOrganizationScope(["kn_0004", "kn_0001"])).rejects.toThrow();
    await expect(api.validateOrganizationScope(["kn_0001", "kn_0001"])).rejects.toThrow();
    await expect(api.validateOrganizationScope(["kn_0001", "missing-id"])).rejects.toThrow();
  });

  it("rejects inactive knowledge at both source and target", async () => {
    const api = new MockAiksApi();
    const candidate = "kn_0005";
    await api.archiveKnowledge(candidate);
    try {
      await expect(api.validateOrganizationScope(["kn_0001", candidate])).rejects.toThrow();
    } finally {
      await api.restoreKnowledge(candidate);
    }
    expect(await api.validateOrganizationScope(["kn_0001", candidate])).toBe(true);
  });

  it("requires source identities in the final create command and leaves ordinary manual creation compatible", async () => {
    const api = new MockAiksApi();
    const input = {
      title: "S3.4 reviewed scoped document",
      content: "# Manually reviewed draft",
      project_name: "Pipeline",
      tags: ["reviewed"],
    };
    await expect(api.createOrganizedKnowledge(input, [], "structure")).rejects.toThrow();
    await expect(api.createOrganizedKnowledge(input, ["kn_0001", "kn_0002"], "compare")).rejects.toThrow();
    expect((await api.getKnowledge({ limit: 200 })).items
      .some(item => item.title === input.title)).toBe(false);
    const created = await api.createOrganizedKnowledge(input, ["kn_0001", "kn_0005"], "compare");
    expect(created.title).toBe(input.title);
    expect(created.source_type).toBe("manual");
    expect(created.managed_by).toBe("user");
  });

  it("calls Rust-side canonical validation at both safety boundaries", () => {
    expect(organizationPage).toContain(
      "await getApi().validateOrganizationScope(sources.map(source => source.id))",
    );
    expect(organizationPage).toContain(
      "await getApi().validateOrganizationScope(currentSources.map(source => source.id))",
    );
    expect(organizationPage).toContain("getApi().createOrganizedKnowledge({");
    expect(organizationPage).toContain("}, organizeSourceIds, reviewedOperation)");
    expect(rustCommand).toContain("pub fn validate_organization_scope(");
    expect(rustCommand).toContain("pub async fn create_organized_knowledge(");
    expect(rustCommand).toContain(".validate(&source_knowledge_ids)");
    expect(rustCommand).toContain("create_knowledge(input, state).await");
    expect(rustCommand).toContain("OrganizationAuditRepo::validate_input");
    expect(rustCommand).toContain("OrganizationScopeGuard::new(&db)");
    expect(coreScope).toContain("COALESCE(pb.project_path,ss.project_path)");
    expect(coreScope).toContain("Organization source is not active");
    expect(coreScope).toContain("Different or unverified source projects cannot be merged");
    expect(coreScope).toContain("is_missing == Some(0)");
  });
});
