mod annotations;
mod app_updates;
mod archive;
mod batch;
mod bookmarks;
mod certificate;
mod child_process;
mod combine;
mod compression;
mod content_editor;
mod document_io;
mod export;
mod file_safety;
mod forms;
mod health;
mod job_control;
mod job_recovery;
mod ocr;
mod ocr_progress;
mod operation_audit;
mod page_finish;
mod pdf_jobs;
mod pdf_tools;
mod pdfx;
mod privacy;
mod privacy_inspection;
mod protection;
mod recovery;
mod redaction;
mod runtime_capabilities;
mod scan_cleanup;
mod scan_export;
mod scanner;
mod signature_vault;
mod temporary_cleanup;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default().plugin(tauri_plugin_dialog::init());

    #[cfg(feature = "e2e")]
    let builder = builder
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());

    builder
        .setup(|app| {
            #[cfg(feature = "e2e")]
            if app.config().identifier != "org.tufekci.paperworks.e2e" {
                return Err(std::io::Error::other(
                    "the e2e feature requires the isolated E2E application identifier",
                )
                .into());
            }

            let update_state =
                app_updates::initialise(app.handle()).map_err(std::io::Error::other)?;
            let cleanup_status =
                temporary_cleanup::initialise(app.handle()).map_err(std::io::Error::other)?;
            let audit = operation_audit::OperationAudit::initialise(app.handle())
                .map_err(std::io::Error::other)?;
            let (job_recovery, recovered_jobs) =
                job_recovery::JobRecoveryStore::initialise(app.handle())
                    .map_err(std::io::Error::other)?;
            let scanner_capture_root = scanner::initialise_scanner_capture_root(app.handle())
                .map_err(std::io::Error::other)?;
            app.manage(cleanup_status);
            app.manage(update_state);
            app.manage(pdf_jobs::PdfJobManager::with_services(
                audit.clone(),
                job_recovery,
                recovered_jobs,
                scanner_capture_root,
            ));
            app.manage(audit);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_updates::check_for_update,
            app_updates::install_update,
            app_updates::restart_after_update,
            app_updates::update_readiness,
            archive::pdf_archive_readiness,
            certificate::certificate_capabilities,
            document_io::read_local_document,
            document_io::open_local_pdf,
            document_io::read_local_pdf_range,
            ocr::ocr_readiness,
            operation_audit::clear_operation_audit,
            operation_audit::export_operation_audit,
            operation_audit::list_operation_audit,
            pdf_jobs::cancel_pdf_job,
            pdf_jobs::get_pdf_job,
            pdf_jobs::list_pdf_jobs,
            pdf_jobs::start_pdf_job,
            pdf_tools::probe_tools,
            pdf_tools::scan_presets,
            pdf_tools::signature_capabilities,
            protection::protection_capabilities,
            recovery::clear_recovery_snapshots,
            recovery::load_recovery_snapshot,
            recovery::save_recovery_snapshot,
            runtime_capabilities::runtime_capabilities,
            scanner::list_scanners,
            signature_vault::delete_signature_vault,
            signature_vault::list_signature_vault,
            signature_vault::store_signature_vault,
            signature_vault::unlock_signature_vault,
            temporary_cleanup::temporary_cleanup_status
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Tüfekci Paperworks");
}
