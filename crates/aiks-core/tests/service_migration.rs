//! Adoption is an explicit transaction on a backed-up, exclusively held local DB.
use std::{sync::Arc, time::Duration};
use aiks_core::{
    ai::AiModelConfig, model::SourceKind,
    pipeline::{job_repo::PipelineJobRepo, EmbeddingConfig, PipelineJob, PipelineOrchestrator, PipelineWorker},
    service::{adopt_local_state, query, AdoptionManifest, SessionAdoption, ServiceError, ServiceStore},
    storage::{SourceSessionRepo, StateDb},
};
#[path = "support/service_fixture.rs"]
mod fixture;

struct Legacy {
    root: tempfile::TempDir,
    db: Arc<StateDb>,
    store: ServiceStore,
    id: i64,
    job: String,
    manifest: AdoptionManifest,
}
impl Legacy {
    fn new(available: bool) -> Self {
        let root=tempfile::tempdir().unwrap();
        let path=root.path().join("state.db");
        // Seed through the pre-service public repositories before a service
        // identity is even created. Every business ID/map is a legacy one.
        let db=Arc::new(StateDb::open(&path).unwrap());
        let id=SourceSessionRepo::new(&db).upsert("continue","legacy-session",None,None,None,
            Some("Legacy title"),None,Some("legacy-hash"),Some("legacy-parser")).unwrap();
        let run=PipelineOrchestrator::new(db.clone()).enqueue(id,Some("legacy-hash")).unwrap();
        let job=PipelineJobRepo::new(&db).enqueue(&PipelineJob {
            pipeline_run_id:run,session_id:id,session_external_id:"legacy-session".into(),
            source:"continue".into(),session_title:Some("Legacy title".into()),project_name:None,
        }).unwrap().durable_job_id;
        {
            let conn=db.conn();
            conn.execute("INSERT INTO knowledge_item (id,source_session_id,title,content,created_at,updated_at,siyuan_doc_id,generated_hash,current_remote_hash,managed_by)
                VALUES ('legacy-knowledge',?1,'User knowledge','KEEP_BODY','old','old','old-doc','base-hash','remote-hash','user')",[id]).unwrap();
            conn.execute("INSERT INTO knowledge_item (id,title,content,created_at,updated_at,siyuan_doc_id,managed_by,source_type)
                VALUES ('manual','Manual published','KEEP_MANUAL','old','old','manual-doc','user','manual')",[]).unwrap();
            conn.execute("INSERT INTO sync_target(session_id,sink,target_id,target_path,synced_hash,target_hash,status)
                VALUES (?1,'siyuan','raw-doc','/old','raw-base','raw-remote','SYNCED')",[id]).unwrap();
            conn.execute("INSERT INTO knowledge_sync_target(knowledge_id,target_id,synced_hash,target_hash,status,updated_at)
                VALUES ('legacy-knowledge','old-doc','base-hash','remote-hash','SYNCED','old')",[]).unwrap();
            conn.execute("INSERT INTO session_search_fts(session_id,external_id,source,title,content)
                VALUES (?1,'legacy-session','continue','Legacy title','LEGACY_PROJECTION')",[id]).unwrap();
        }
        drop(db);
        let db=Arc::new(StateDb::open_exclusive(&path).unwrap());
        let store=ServiceStore::open(db.clone()).unwrap();
        let ctx=store.local_context();
        let reg=store.register_source(&ctx,SourceKind::Continue,"legacy-device").unwrap();
        let mut input=fixture::submission(ctx.space_id(),ctx.instance_id(),&reg,"adopt-1",0,"ADOPTED_BODY");
        input.session.external_session_id="legacy-session".into();
        let manifest=AdoptionManifest {
            service_instance_id:ctx.instance_id().into(),
            sessions:vec![SessionAdoption {session_id:id,source:SourceKind::Continue,upstream_id:"legacy-session".into(),
                registration_id:reg,snapshot:available.then_some(input)}],
            knowledge_ids:vec!["manual".into()],
        };
        Self{root,db,store,id,job,manifest}
    }
    fn apply(&self)->Result<aiks_core::service::AdoptionReport,ServiceError>{
        adopt_local_state(&self.db,&self.store.local_context(),&self.manifest)
    }
    fn count(&self,table:&str)->i64 {
        self.db.conn().query_row(&format!("SELECT COUNT(*) FROM {table}"),[],|row|row.get(0)).unwrap()
    }
    fn preserved(&self) {
        let conn=self.db.conn();
        let source:(i64,String,String)=conn.query_row("SELECT id,source,external_session_id FROM source_session WHERE id=?1",[self.id],
            |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
        assert_eq!(source,(self.id,"continue".into(),"legacy-session".into()));
        let knowledge:(String,String,String,String)=conn.query_row("SELECT content,siyuan_doc_id,generated_hash,current_remote_hash FROM knowledge_item WHERE id='legacy-knowledge'",[],
            |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
        assert_eq!(knowledge,("KEEP_BODY".into(),"old-doc".into(),"base-hash".into(),"remote-hash".into()));
        let maps:(String,String,String)=conn.query_row("SELECT target_id,synced_hash,target_hash FROM knowledge_sync_target WHERE knowledge_id='legacy-knowledge'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
        assert_eq!(maps,("old-doc".into(),"base-hash".into(),"remote-hash".into()));
        let raw:(String,String,String)=conn.query_row("SELECT target_id,synced_hash,target_hash FROM sync_target WHERE session_id=?1",[self.id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
        assert_eq!(raw,("raw-doc".into(),"raw-base".into(),"raw-remote".into()));
    }
}

#[test]
fn adoption_preserves_canonical_ids_mappings_and_is_idempotent() {
    let f=Legacy::new(true);
    let report=f.apply().unwrap();
    assert!(report.conflict.is_empty());
    assert_eq!(report.snapshot_ready,vec![f.id]);
    let again=f.apply().unwrap();
    assert_eq!(report.receipts,again.receipts);
    assert_eq!(f.count("source_session"),1);
    assert_eq!(f.count("service_session_snapshot"),1);
    assert_eq!(f.count("service_ingest_receipt"),1);
    assert_eq!(f.count("pipeline_job"),2);
    f.preserved();
    let ctx=f.store.local_context();
    let old=query::knowledge(&f.db,&ctx,"legacy-knowledge").unwrap();
    assert_eq!(old.revision,None);
    assert!(old.stale);
    assert!(query::knowledge(&f.db,&ctx,"manual").is_ok());
}

#[test]
fn missing_source_is_readable_but_never_fabricated_or_reprocessed() {
    let f=Legacy::new(false);
    let report=f.apply().unwrap();
    assert_eq!(report.source_unavailable,vec![f.id]);
    assert_eq!(f.count("service_session_snapshot"),0);
    assert_eq!(f.count("pipeline_job"),1);
    let job:String=f.db.conn().query_row("SELECT status FROM pipeline_job WHERE id=?1",[&f.job],|r|r.get(0)).unwrap();
    assert_eq!(job,"PENDING");
    let view=query::session(&f.db,&f.store.local_context(),&f.id.to_string()).unwrap();
    assert_eq!(view["snapshot_state"],"source_unavailable");
    assert_eq!(view["content_state"],"legacy_projection");
    assert_eq!(view["content"],"LEGACY_PROJECTION");
    assert!(view["session"].is_null());
    f.preserved();
}

#[test]
fn mixed_manifest_identity_conflict_makes_no_partial_bindings() {
    let mut f=Legacy::new(false);
    let id=SourceSessionRepo::new(&f.db).upsert("continue","another",None,None,None,None,None,None,None).unwrap();
    let mut wrong=f.manifest.sessions[0].clone();
    wrong.session_id=id;
    f.manifest.sessions.push(wrong);
    let report=f.apply().unwrap();
    assert_eq!(report.conflict,vec![id]);
    assert!(report.bound.is_empty());
    assert_eq!(f.count("service_session_binding"),0);
    assert!(query::knowledge(&f.db,&f.store.local_context(),"manual").is_err());
    f.preserved();
}

#[test]
fn failure_after_binding_rolls_back_and_retry_is_safe() {
    let f=Legacy::new(true);
    f.db.conn().execute_batch("CREATE TRIGGER break_snapshot BEFORE INSERT ON service_session_snapshot BEGIN SELECT RAISE(ABORT,'synthetic'); END;").unwrap();
    assert!(f.apply().is_err());
    assert_eq!(f.count("service_session_binding"),0);
    assert_eq!(f.count("service_session_snapshot"),0);
    assert_eq!(f.count("pipeline_job"),1);
    f.db.conn().execute_batch("DROP TRIGGER break_snapshot").unwrap();
    assert_eq!(f.apply().unwrap().snapshot_ready,vec![f.id]);
    f.preserved();
}

#[test]
fn wrong_instance_registration_or_nonexclusive_handle_never_adopts_data() {
    let mut f=Legacy::new(false);
    f.manifest.service_instance_id="foreign-instance".into();
    assert_eq!(f.apply().unwrap_err(),ServiceError::Unauthorized);
    f.manifest.service_instance_id=f.store.local_context().instance_id().into();
    f.manifest.sessions[0].registration_id="foreign-registration".into();
    assert_eq!(f.apply().unwrap_err(),ServiceError::NotFound);
    let path=f.root.path().join("other.db");
    let unleased=StateDb::open(&path).unwrap();
    assert!(adopt_local_state(&unleased,&f.store.local_context(),&f.manifest).is_err());
    assert_eq!(f.count("service_session_binding"),0);
}

#[test]
fn active_legacy_writer_and_incomplete_snapshot_block_adoption() {
    let mut f=Legacy::new(true);
    f.manifest.sessions[0].snapshot.as_mut().unwrap().complete=false;
    assert_eq!(f.apply().unwrap_err(),ServiceError::IncompleteSnapshot);
    f.manifest.sessions[0].snapshot.as_mut().unwrap().complete=true;
    f.db.conn().execute("UPDATE pipeline_job SET status='RUNNING' WHERE id=?1",[&f.job]).unwrap();
    assert_eq!(f.apply().unwrap_err(),ServiceError::Conflict);
    assert_eq!(f.count("service_session_binding"),0);
}

#[tokio::test]
async fn adopted_snapshot_runs_after_reopening_with_no_provider_or_source_file() {
    let f=Legacy::new(true);
    let report=f.apply().unwrap();
    let job=report.receipts[0].job_id.clone();
    let Legacy{root,db,store,..}=f;
    drop(store);drop(db);
    let db=Arc::new(StateDb::open_exclusive(&root.path().join("state.db")).unwrap());
    let worker=PipelineWorker::start_from_snapshots(db.clone(),AiModelConfig{enabled:false,..Default::default()},
        EmbeddingConfig{enabled:false,..Default::default()});
    tokio::time::timeout(Duration::from_secs(10),async {
        loop {
            let status:String=db.conn().query_row("SELECT status FROM pipeline_job WHERE id=?1",[&job],|r|r.get(0)).unwrap();
            if status=="DONE" {break;}
            assert!(!matches!(status.as_str(),"FAILED"|"SUPERSEDED"));
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    let text:String=db.conn().query_row("SELECT content FROM session_search_fts",[],|r|r.get(0)).unwrap();
    assert!(text.contains("ADOPTED_BODY"));
    worker.shutdown(Duration::from_secs(2)).await.unwrap();
}
