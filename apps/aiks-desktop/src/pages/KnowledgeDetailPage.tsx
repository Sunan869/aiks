import { useCallback, useEffect, useState } from "react";
import { Archive, Bot, ExternalLink, FilePenLine, Loader2, Pencil, RotateCcw, Star } from "lucide-react";
import { getApi } from "../api/client";
import type { KnowledgeDetail, KnowledgeFeedback, KnowledgeFeedbackKind, KnowledgeRelation, KnowledgeRelationType, KnowledgeSummary } from "../api/types";
import KnowledgeEditor from "../components/KnowledgeEditor";
import type { AiAssistOperation } from "../api/ai-assist";

interface Props {
  knowledgeId: string;
  onBack: () => void;
  onViewSession: (sessionId: number) => void;
  onOpenKnowledge?: (knowledgeId: string) => void;
}

function parseTags(tags: string): string[] {
  try { return JSON.parse(tags); } catch { return tags ? [tags] : []; }
}

const CATEGORY_LABELS: Record<string, string> = {
  troubleshooting: "故障排查", architecture: "架构设计", implementation: "实现方案",
  configuration: "配置管理", research: "技术探索", decision: "决策记录", general: "通用",
};

export default function KnowledgeDetailPage({ knowledgeId, onBack, onViewSession, onOpenKnowledge }: Props) {
  const [data, setData] = useState<KnowledgeDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [editing, setEditing] = useState(false);
  const [action, setAction] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<KnowledgeFeedback[]>([]);
  const [feedbackKind, setFeedbackKind] = useState<KnowledgeFeedbackKind>("useful");
  const [feedbackNote, setFeedbackNote] = useState("");
  const [feedbackBusy, setFeedbackBusy] = useState(false);
  const [feedbackError, setFeedbackError] = useState<string | null>(null);
  const [relations, setRelations] = useState<KnowledgeRelation[]>([]);
  const [relationCandidates, setRelationCandidates] = useState<KnowledgeSummary[]>([]);
  const [relationTarget, setRelationTarget] = useState("");
  const [relationType, setRelationType] = useState<KnowledgeRelationType>("related");
  const [relationEvidence, setRelationEvidence] = useState("");
  const [relationBusy, setRelationBusy] = useState(false);
  const [relationError, setRelationError] = useState<string | null>(null);
  const [organizeOperation, setOrganizeOperation] = useState<AiAssistOperation>("structure");
  const [organizeDraft, setOrganizeDraft] = useState("");
  const [organizeBusy, setOrganizeBusy] = useState(false);
  const [organizeConfirmed, setOrganizeConfirmed] = useState(false);
  const [organizeError, setOrganizeError] = useState<string | null>(null);
  const [selectedSources, setSelectedSources] = useState<string[]>([]);
  const [organizeSourceIds, setOrganizeSourceIds] = useState<string[]>([]);
  const [createdKnowledgeId, setCreatedKnowledgeId] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setData(await getApi().getKnowledgeDetail(knowledgeId));
    } finally {
      setLoading(false);
    }
  }, [knowledgeId]);

  useEffect(() => { void load(); }, [load]);
  useEffect(() => {
    let active = true;
    setFeedback([]);
    setFeedbackError(null);
    void getApi().getKnowledgeFeedback(knowledgeId).then(items => {
      if (active) setFeedback(items);
    }).catch(error => {
      if (active) setFeedbackError("反馈加载失败：" + String(error));
    });
    return () => { active = false; };
  }, [knowledgeId]);

  const submitFeedback = async () => {
    setFeedbackBusy(true);
    setFeedbackError(null);
    try {
      const created = await getApi().addKnowledgeFeedback(knowledgeId, feedbackKind, feedbackNote.trim());
      setFeedback(current => [created, ...current]);
      setFeedbackNote("");
    } catch (error) {
      setFeedbackError("反馈保存失败：" + String(error));
    } finally {
      setFeedbackBusy(false);
    }
  };

  useEffect(() => {
    let active = true;
    setRelations([]);
    setRelationError(null);
    void Promise.all([
      getApi().getKnowledgeRelations(knowledgeId),
      getApi().getKnowledge({ limit: 200 }),
    ]).then(([items, page]) => {
      if (active) {
        setRelations(items);
        setRelationCandidates(page.items);
      }
    }).catch(error => {
      if (active) setRelationError("关系加载失败：" + String(error));
    });
    return () => { active = false; };
  }, [knowledgeId]);

  const suggestRelation = async () => {
    setRelationBusy(true);
    setRelationError(null);
    try {
      const newRelation = await getApi().suggestKnowledgeRelation(
        knowledgeId, relationTarget.trim(), relationType, relationEvidence.trim()
      );
      setRelations(current => [newRelation, ...current]);
      setRelationTarget("");
      setRelationEvidence("");
    } catch (error) {
      setRelationError("创建关系失败：" + String(error));
    } finally {
      setRelationBusy(false);
    }
  };

  const reviewRelation = async (id: string, decision: "confirmed" | "rejected") => {
    setRelationBusy(true);
    setRelationError(null);
    try {
      const updated = await getApi().reviewKnowledgeRelation(id, decision);
      setRelations(current => current.map(item => item.id === id ? updated : item));
    } catch (error) {
      setRelationError("关系审核失败：" + String(error));
    } finally {
      setRelationBusy(false);
    }
  };

  const suggestOrganization = async () => {
    if (!data?.siyuan_doc_id) return;
    if ((organizeOperation === "compare" || organizeOperation === "merge_draft") && selectedSources.length === 0) {
      setOrganizeError("多来源对比或合并至少需要另外选中一条知识");
      return;
    }
    setOrganizeBusy(true);
    setOrganizeError(null);
    setOrganizeConfirmed(false);
    setOrganizeDraft("");
    setCreatedKnowledgeId(null);
    try {
      const others = await Promise.all(
        selectedSources.slice(0, 4).map(id => getApi().getKnowledgeDetail(id))
      );
      const sources = [data, ...others];
      const sourceIds = sources.map(item => item.id);
      const sourceContext = sources.map(item => (
        "## 资料：" + item.title + "（知识 ID：" + item.id + "）\n\n" + item.content
      )).join("\n\n---\n\n");
      const suggestion = await getApi().assistKnowledge({
        siyuanDocId: data.siyuan_doc_id,
        operation: organizeOperation,
        title: data.title,
        content: sourceContext,
        existingSummary: data.summary,
        existingTags: parseTags(data.tags),
        existingCategory: data.category,
      });
      if (!suggestion.text?.trim()) throw new Error("模型未返回可审核的内容");
      setOrganizeSourceIds(sourceIds);
      setOrganizeDraft(suggestion.text);
    } catch (error) {
      setOrganizeError("生成建议失败：" + String(error));
    } finally {
      setOrganizeBusy(false);
    }
  };

  const createDerivedKnowledge = async () => {
    if (!data || !organizeConfirmed || !organizeDraft.trim() || organizeBusy) return;
    setOrganizeBusy(true);
    setOrganizeError(null);
    try {
      const sourceLinks = organizeSourceIds.map(id => "- AIKS 知识：" + id).join("\n");
      const content = organizeDraft + "\n\n---\n\n来源（经用户确认的整理草稿，原知识未修改）：\n" + sourceLinks;
      const labels: Record<string, string> = {
        compare: "来源对比",
        merge_draft: "多来源整理",
        structure: "结构整理",
        rewrite: "润色草稿",
        key_conclusions: "关键结论",
      };
      const created = await getApi().createKnowledge({
        title: data.title + " · " + (labels[organizeOperation] ?? "整理草稿"),
        category: data.category,
        project_name: data.project_name,
        summary: "由用户确认的 AIKS 知识整理结果；请通过来源知识 ID 核验。",
        tags: parseTags(data.tags),
        content,
      });
      setCreatedKnowledgeId(created.id);
      setMessage("已创建独立知识文档，原文保持不变。可从下方打开或归档本次输出。");
    } catch (error) {
      setOrganizeError("创建独立知识失败：" + String(error));
    } finally {
      setOrganizeBusy(false);
    }
  };

  const undoDerivedKnowledge = async () => {
    if (!createdKnowledgeId || organizeBusy) return;
    setOrganizeBusy(true);
    setOrganizeError(null);
    try {
      await getApi().archiveKnowledge(createdKnowledgeId);
      setMessage("已将本次生成的知识文档归档；来源知识仍保留。");
      setCreatedKnowledgeId(null);
    } catch (error) {
      setOrganizeError("归档生成的知识失败：" + String(error));
    } finally {
      setOrganizeBusy(false);
    }
  };

  const copyOrganizationDraft = async () => {
    if (!organizeConfirmed || !organizeDraft.trim()) return;
    try {
      await navigator.clipboard.writeText(organizeDraft);
      setMessage("已复制审核后的草稿。请在 SiYuan 中人工对比原文后编辑，AIKS 未自动写入。");
    } catch (error) {
      setOrganizeError("复制失败：" + String(error));
    }
  };

  const runAction = async (name: string, fn: () => Promise<KnowledgeDetail>) => {
    setAction(name);
    setMessage(null);
    try {
      setData(await fn());
    } catch (e) {
      setMessage(`操作失败：${String(e)}`);
    } finally {
      setAction(null);
    }
  };

  const publish = async () => {
    if (!data) return;
    setAction("publish");
    setMessage(null);
    try {
      const result = await getApi().publishKnowledge(data.id);
      const labels: Record<string, string> = {
        created: "已发布到 SiYuan",
        updated: "已更新 SiYuan 文档",
        unchanged: "SiYuan 中已是最新版本",
        conflict: "检测到 SiYuan 端人工修改，未覆盖远端内容",
      };
      setMessage(labels[result.outcome] ?? `SiYuan 发布结果：${result.outcome}`);
    } catch (e) {
      setMessage(`发布失败：${String(e)}`);
    } finally {
      setAction(null);
    }
  };

  if (loading) return <div className="flex h-48 items-center justify-center p-6 text-sm text-gray-400">加载中...</div>;
  if (!data) return <div className="p-6 text-gray-400">未找到知识条目：{knowledgeId}</div>;

  const tags = parseTags(data.tags);

  return (
    <div className="p-6">
      <div className="mb-5 flex items-center gap-2">
        <button onClick={onBack} className="text-sm text-blue-500 hover:text-blue-700">← 返回</button>
        <span className="text-gray-300">/</span>
        <span className="text-sm text-gray-600 dark:text-gray-400">知识详情</span>
      </div>

      <div className="mb-5 rounded-lg border border-gray-200 bg-white p-5 dark:border-gray-700 dark:bg-gray-800">
        <div className="flex items-start justify-between gap-5">
          <div className="min-w-0 flex-1">
            <div className="mb-2 flex flex-wrap items-center gap-2">
              {data.source_type === "manual" ? (
                <span className="inline-flex items-center gap-1 rounded bg-emerald-50 px-2 py-1 text-xs font-medium text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-300"><FilePenLine className="h-3.5 w-3.5" />手动创建</span>
              ) : (
                <span className="inline-flex items-center gap-1 rounded bg-blue-50 px-2 py-1 text-xs font-medium text-blue-700 dark:bg-blue-900/30 dark:text-blue-300"><Bot className="h-3.5 w-3.5" />AI 提炼</span>
              )}
              <span className="rounded bg-gray-100 px-2 py-1 text-xs text-gray-600 dark:bg-gray-700 dark:text-gray-300">{CATEGORY_LABELS[data.category] ?? data.category}</span>
              {data.managed_by === "user" && data.source_type === "conversation" && <span className="rounded bg-amber-50 px-2 py-1 text-xs text-amber-700 dark:bg-amber-900/30 dark:text-amber-300">用户管理 · AI 不再覆盖</span>}
              {data.status === "archived" && <span className="rounded bg-gray-100 px-2 py-1 text-xs text-gray-500">已归档</span>}
            </div>
            <h1 className="text-xl font-semibold text-gray-900 dark:text-gray-100">{data.title}</h1>
            {data.summary && <p className="mt-3 text-sm leading-6 text-gray-500 dark:text-gray-400">{data.summary}</p>}
          </div>

          <div className="flex flex-wrap justify-end gap-2">
            <button onClick={() => setEditing(true)} className="inline-flex items-center gap-1.5 rounded-md border border-gray-200 px-3 py-1.5 text-xs text-gray-600 hover:bg-gray-50 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-700"><Pencil className="h-3.5 w-3.5" />编辑</button>
            <button
              onClick={() => void runAction("favorite", () => getApi().setKnowledgeFavorite(data.id, !data.is_favorite))}
              disabled={action !== null}
              className={`inline-flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-xs ${data.is_favorite ? "border-amber-200 bg-amber-50 text-amber-700" : "border-gray-200 text-gray-600 dark:border-gray-700 dark:text-gray-300"}`}
            >
              {action === "favorite" ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Star className="h-3.5 w-3.5" fill={data.is_favorite ? "currentColor" : "none"} />}
              {data.is_favorite ? "已收藏" : "收藏"}
            </button>
            {data.status === "archived" ? (
              <button onClick={() => void runAction("restore", () => getApi().restoreKnowledge(data.id))} disabled={action !== null} className="inline-flex items-center gap-1.5 rounded-md border border-gray-200 px-3 py-1.5 text-xs text-gray-600 dark:border-gray-700 dark:text-gray-300"><RotateCcw className="h-3.5 w-3.5" />恢复</button>
            ) : (
              <button onClick={() => void runAction("archive", () => getApi().archiveKnowledge(data.id))} disabled={action !== null} className="inline-flex items-center gap-1.5 rounded-md border border-gray-200 px-3 py-1.5 text-xs text-gray-600 dark:border-gray-700 dark:text-gray-300"><Archive className="h-3.5 w-3.5" />归档</button>
            )}
            <button onClick={() => void publish()} disabled={action !== null || data.status === "archived"} className="inline-flex items-center gap-1.5 rounded-md border border-violet-200 bg-violet-50 px-3 py-1.5 text-xs text-violet-700 hover:bg-violet-100 disabled:opacity-40 dark:border-violet-800 dark:bg-violet-900/20 dark:text-violet-300">
              {action === "publish" ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <ExternalLink className="h-3.5 w-3.5" />}
              发布到 SiYuan
            </button>
          </div>
        </div>

        <div className="mt-4 flex flex-wrap gap-1.5">
          {data.project_name && <span className="rounded bg-gray-100 px-2 py-0.5 text-xs text-gray-500 dark:bg-gray-700">项目：{data.project_name}</span>}
          {tags.map(tag => <span key={tag} className="rounded bg-gray-100 px-2 py-0.5 text-xs text-gray-500 dark:bg-gray-700">#{tag}</span>)}
        </div>

        <div className="mt-4 border-t border-gray-100 pt-3 dark:border-gray-700">
          {data.source_type === "manual" ? (
            <div className="flex items-center gap-2 text-xs text-gray-400"><FilePenLine className="h-3.5 w-3.5" />这条知识由你直接在 AIKS 中创建，没有绑定外部工作记录。</div>
          ) : (
            <div className="flex items-center justify-between gap-3">
              <div className="text-xs text-gray-400">来源：<span className="font-mono">{data.session_external_id}</span>{data.session_title && <span className="ml-1">· {data.session_title}</span>}</div>
              {data.session_id !== null && <button onClick={() => onViewSession(data.session_id as number)} className="rounded border border-gray-200 px-2.5 py-1 text-xs text-gray-600 hover:bg-gray-50 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-700">查看原始工作记录</button>}
            </div>
          )}
        </div>
      </div>

      {message && <div className={`mb-4 rounded-md border px-4 py-2 text-xs ${message.includes("失败") ? "border-red-200 bg-red-50 text-red-700" : message.includes("冲突") ? "border-amber-200 bg-amber-50 text-amber-700" : "border-green-200 bg-green-50 text-green-700"}`}>{message}</div>}

      <div className="mb-5 rounded-lg border border-gray-200 bg-white p-5 dark:border-gray-700 dark:bg-gray-800">
        <div className="mb-3 flex items-center justify-between">
          <h2 className="text-sm font-medium text-gray-700 dark:text-gray-300">知识内容</h2>
          <span className="text-[10px] text-gray-400">Markdown</span>
        </div>
        <pre className="whitespace-pre-wrap font-sans text-sm leading-7 text-gray-700 dark:text-gray-300">{data.content}</pre>
      </div>

      <section className="mb-5 rounded-lg border border-gray-200 bg-white p-5 dark:border-gray-700 dark:bg-gray-800">
        <h2 className="mb-2 text-sm font-medium text-gray-700 dark:text-gray-300">跨 Session 知识演进关系</h2>
        <p className="mb-3 text-xs text-gray-500">所有关系先记录为建议，人工确认后才成为已确认的知识关联；原知识不会被自动删除或覆盖。</p>
        <div className="flex flex-wrap gap-2">
          <input aria-label="关联知识 ID" list="knowledge-relation-candidates" value={relationTarget} onChange={event => setRelationTarget(event.target.value)} placeholder="目标知识 ID" className="min-w-0 flex-1 rounded border border-gray-200 bg-transparent p-2 text-xs dark:border-gray-600" />
          <datalist id="knowledge-relation-candidates">
            {relationCandidates.filter(item => item.id !== knowledgeId && (!data.project_name || !item.project_name || item.project_name === data.project_name)).map(item => <option key={item.id} value={item.id}>{item.title}</option>)}
          </datalist>
          <select aria-label="关联类型" value={relationType} onChange={event => setRelationType(event.target.value as KnowledgeRelationType)} className="rounded border border-gray-200 bg-transparent p-2 text-xs dark:border-gray-600">
            <option value="related">相关</option>
            <option value="supplements">补充</option>
            <option value="corrects">纠正</option>
            <option value="supersedes">替代</option>
            <option value="resolved_by">由其解决</option>
          </select>
        </div>
        <textarea aria-label="关联证据" value={relationEvidence} onChange={event => setRelationEvidence(event.target.value)} maxLength={4000} rows={2} placeholder="写明来源 Session、消息区间或判断依据（必填）" className="mt-2 w-full rounded border border-gray-200 bg-transparent p-2 text-xs dark:border-gray-600" />
        <button type="button" disabled={relationBusy || !relationTarget.trim() || !relationEvidence.trim()} onClick={() => void suggestRelation()} className="rounded bg-blue-600 px-3 py-2 text-xs text-white disabled:opacity-50">添加关系建议</button>
        {relationError && <p className="mt-2 text-xs text-red-600" role="alert">{relationError}</p>}
        <div className="mt-3 space-y-2">
          {relations.map(relation => {
            const counterpartId = relation.source_id === knowledgeId ? relation.target_id : relation.source_id;
            const counterpart = relationCandidates.find(item => item.id === counterpartId);
            return (
              <div key={relation.id} className="border-t border-gray-100 pt-2 text-xs dark:border-gray-700">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="font-medium">{relation.relation_type}</span>
                  <span className="text-gray-500">{counterpart?.title ?? counterpartId}</span>
                  <span className="text-gray-400">{relation.status === "suggested" ? "待确认" : relation.status === "confirmed" ? "已确认" : "已拒绝"}</span>
                  <span className="text-gray-400">{new Date(relation.updated_at).toLocaleString("zh-CN")}</span>
                  {relation.status !== "confirmed" && <button type="button" disabled={relationBusy} onClick={() => void reviewRelation(relation.id, "confirmed")} className="text-blue-600">确认</button>}
                  {relation.status !== "rejected" && <button type="button" disabled={relationBusy} onClick={() => void reviewRelation(relation.id, "rejected")} className="text-red-600">撤销/拒绝</button>}
                </div>
                <p className="mt-1 whitespace-pre-wrap text-gray-600 dark:text-gray-300">{relation.evidence}</p>
              </div>
            );
          })}
        </div>
      </section>

      <section className="mb-5 rounded-lg border border-gray-200 bg-white p-5 dark:border-gray-700 dark:bg-gray-800">
        <h2 className="mb-2 text-sm font-medium text-gray-700 dark:text-gray-300">知识质量反馈</h2>
        <p className="mb-3 text-xs text-gray-500">反馈只保存在本地，不会自动删除知识或覆盖人工编辑。</p>
        <div className="flex flex-wrap gap-2">
          <select aria-label="反馈类型" value={feedbackKind} onChange={event => setFeedbackKind(event.target.value as KnowledgeFeedbackKind)} className="rounded border border-gray-200 bg-transparent p-2 text-sm dark:border-gray-600">
            <option value="useful">有用</option>
            <option value="incorrect">错误</option>
            <option value="duplicate">重复</option>
            <option value="outdated">过时</option>
            <option value="needs_detail">需补充</option>
          </select>
          <input aria-label="反馈说明" value={feedbackNote} maxLength={4000} onChange={event => setFeedbackNote(event.target.value)} placeholder="说明或修正建议（可选）" className="min-w-0 flex-1 rounded border border-gray-200 bg-transparent p-2 text-sm dark:border-gray-600" />
          <button type="button" disabled={feedbackBusy} onClick={() => void submitFeedback()} className="rounded bg-blue-600 px-3 py-2 text-sm text-white disabled:opacity-50">{feedbackBusy ? "保存中..." : "提交反馈"}</button>
        </div>
        {feedbackError && <p className="mt-2 text-xs text-red-600" role="alert">{feedbackError}</p>}
        <div className="mt-3 space-y-2">
          {feedback.map(item => (
            <div key={item.id} className="border-t border-gray-100 pt-2 text-xs dark:border-gray-700">
              <span className="font-medium">{({ useful: "有用", incorrect: "错误", duplicate: "重复", outdated: "过时", needs_detail: "需补充" } as Record<KnowledgeFeedbackKind, string>)[item.kind]}</span>
              <span className="ml-2 text-gray-400">{new Date(item.created_at).toLocaleString("zh-CN")}</span>
              {item.note && <p className="mt-1 whitespace-pre-wrap text-gray-600 dark:text-gray-300">{item.note}</p>}
            </div>
          ))}
        </div>
      </section>

      <section className="mb-5 rounded-lg border border-gray-200 bg-white p-5 dark:border-gray-700 dark:bg-gray-800">
        <h2 className="mb-2 text-sm font-medium text-gray-700 dark:text-gray-300">受控知识整理 · 草稿预览</h2>
        <p className="mb-3 text-xs text-gray-500">AI 整理建议不会直接覆盖 SiYuan；先对比原文，确认后仅复制草稿，由你在 SiYuan 中手工应用。</p>
        <div className="flex flex-wrap gap-2">
          <select aria-label="整理方式" value={organizeOperation} onChange={event => {
            setOrganizeOperation(event.target.value as AiAssistOperation);
            setOrganizeDraft("");
            setOrganizeConfirmed(false);
          }} className="rounded border border-gray-200 bg-transparent p-2 text-xs dark:border-gray-700">
            <option value="structure">结构化整理</option>
            <option value="rewrite">语言润色</option>
            <option value="key_conclusions">提取关键结论</option>
            <option value="compare">比较多份知识</option>
            <option value="merge_draft">合并为新文档草稿</option>
          </select>
          {(organizeOperation === "compare" || organizeOperation === "merge_draft") && (
            <label className="flex flex-col gap-1 text-xs">
              <span>可选的其他知识来源（最多 4 条，Ctrl/Command 可多选）</span>
              <select multiple size={4} aria-label="多来源知识选择" value={selectedSources}
                onChange={event => {
                  setSelectedSources(Array.from(event.target.selectedOptions).map(option => option.value).slice(0, 4));
                  setOrganizeConfirmed(false);
                  setOrganizeDraft("");
                }}
                className="w-full rounded border border-gray-200 bg-transparent p-2 dark:border-gray-700">
                {relationCandidates.filter(item => item.id !== knowledgeId &&
                  (!data.project_name || !item.project_name || data.project_name === item.project_name))
                  .map(item => <option key={item.id} value={item.id}>{item.title}</option>)}
              </select>
            </label>
          )}
          <button type="button" disabled={organizeBusy || !data.siyuan_doc_id}
            onClick={() => void suggestOrganization()}
            className="rounded bg-indigo-600 px-3 py-2 text-xs text-white disabled:opacity-40">
            {organizeBusy ? "生成中..." : "生成整理建议"}
          </button>
        </div>
        {!data.siyuan_doc_id && <p className="mt-2 text-xs text-amber-600">该知识尚无 SiYuan 文档，暂不能调用整理助手。</p>}
        {organizeError && <p role="alert" className="mt-2 text-xs text-red-600">{organizeError}</p>}
        {organizeDraft && (
          <div className="mt-3 space-y-3">
            <div className="grid gap-2 lg:grid-cols-2">
              <div>
                <label className="mb-1 block text-xs font-medium">当前正文（只读）</label>
                <textarea aria-label="当前知识正文" value={data.content} readOnly rows={12}
                  className="w-full rounded border bg-gray-50 p-2 font-mono text-xs dark:border-gray-700 dark:bg-gray-900" />
              </div>
              <div>
                <label className="mb-1 block text-xs font-medium">建议草稿（可编辑）</label>
                <textarea aria-label="知识整理草稿" value={organizeDraft} maxLength={250000}
                  onChange={event => { setOrganizeDraft(event.target.value); setOrganizeConfirmed(false); }}
                  rows={12} className="w-full rounded border bg-transparent p-2 font-mono text-xs dark:border-gray-700" />
              </div>
            </div>
            <label className="flex items-center gap-2 text-xs">
              <input type="checkbox" checked={organizeConfirmed} onChange={event => setOrganizeConfirmed(event.target.checked)} />
              我已经核对原文与草稿，理解复制不会修改 SiYuan 中的知识。
            </label>
            <button type="button" disabled={!organizeConfirmed || !organizeDraft.trim()}
              onClick={() => void copyOrganizationDraft()}
              className="rounded border border-indigo-300 px-3 py-2 text-xs text-indigo-700 disabled:opacity-40">
              复制已审核的 Markdown
            </button>
            <button type="button" disabled={!organizeConfirmed || !organizeDraft.trim() || organizeBusy || Boolean(createdKnowledgeId)}
              onClick={() => void createDerivedKnowledge()}
              className="ml-2 rounded border border-emerald-300 px-3 py-2 text-xs text-emerald-700 disabled:opacity-40">
              创建为新的知识文档（不覆盖原文）
            </button>
            {createdKnowledgeId && (
              <div className="flex flex-wrap items-center gap-3 text-xs">
                {onOpenKnowledge && <button type="button" className="text-blue-600" onClick={() => onOpenKnowledge(createdKnowledgeId)}>打开新知识</button>}
                <button type="button" disabled={organizeBusy} className="text-amber-700" onClick={() => void undoDerivedKnowledge()}>撤销本次整理（归档新知识）</button>
              </div>
            )}
          </div>
        )}
      </section>

      <div className="flex gap-4 text-xs text-gray-400">
        <span>创建：{new Date(data.created_at).toLocaleString("zh-CN")}</span>
        <span>更新：{new Date(data.updated_at).toLocaleString("zh-CN")}</span>
        <span>{data.source_type === "manual" ? "本地手工知识" : `${Math.round(data.confidence * 100)}% AI 置信度`}</span>
      </div>

      <KnowledgeEditor
        open={editing}
        initial={data}
        onClose={() => setEditing(false)}
        onSaved={saved => {
          setEditing(false);
          setData(saved);
          setMessage("知识已保存");
        }}
      />
    </div>
  );
}
