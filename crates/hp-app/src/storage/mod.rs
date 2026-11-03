mod delete;
mod job;
mod locations;
mod scan;
#[cfg(test)]
mod tests;

pub use delete::{DeleteOutcome, ItemError, delete_rows, remove_tree};
pub use job::{DeleteSummary, StorageRuntime};
pub use locations::{AppDirs, Filter, Part, Risk, Root, Spec, TABLE, find_spec, in_use};
pub use scan::{Entry, Progress, Report, format_bytes, format_bytes_in, scan_ids, scan_report};

use crate::i18n::TrTable;

pub const TR: TrTable = &[
    ("storage.row.settings", "Settings", "Réglages"),
    (
        "storage.row.settings.desc",
        "Your saved options, key bindings and recent maps. Deleting resets Hammer Pro to its defaults.",
        "Vos options, raccourcis et maps récentes. Les supprimer remet Hammer Pro à ses valeurs par défaut.",
    ),
    ("storage.row.prefabs", "Prefabs", "Prefabs"),
    (
        "storage.row.prefabs.desc",
        "Prefab maps you created in the default prefab folder. They cannot be recovered.",
        "Prefabs que vous avez créés dans le dossier par défaut. Ils ne peuvent pas être récupérés.",
    ),
    ("storage.row.sprinkle", "Sprinkle presets", "Préréglages de saupoudrage"),
    (
        "storage.row.sprinkle.desc",
        "Your saved sprinkle tool presets. They cannot be recovered.",
        "Vos préréglages de l'outil de saupoudrage. Ils ne peuvent pas être récupérés.",
    ),
    ("storage.row.map_state", "Per map state", "État par map"),
    (
        "storage.row.map_state.desc",
        "Saved view, visibility and layout of each map you opened. Maps themselves are not touched.",
        "Vue, visibilité et disposition enregistrées pour chaque map ouverte. Les maps ne sont pas touchées.",
    ),
    ("storage.row.autosave", "Autosaves and backups", "Sauvegardes automatiques"),
    (
        "storage.row.autosave.desc",
        "Recovery copies of maps being edited. Deleting them removes the chance to recover unsaved work.",
        "Copies de secours des maps en cours d'édition. Les supprimer retire la possibilité de récupérer un travail non enregistré.",
    ),
    ("storage.row.crash", "Crash reports", "Rapports de plantage"),
    (
        "storage.row.crash.desc",
        "Crash reports and the emergency saves written with them.",
        "Rapports de plantage et sauvegardes d'urgence écrites avec eux.",
    ),
    ("storage.row.logs", "Logs", "Journaux"),
    (
        "storage.row.logs.desc",
        "Log files of the previous run (the log being written is kept).",
        "Journaux de l'exécution précédente (le journal en cours d'écriture est conservé).",
    ),
    ("storage.row.thumbnails", "Thumbnails", "Miniatures"),
    (
        "storage.row.thumbnails.desc",
        "Cached material, model and prefab thumbnails. They are drawn again when needed.",
        "Miniatures de matériaux, modèles et prefabs en cache. Elles sont redessinées au besoin.",
    ),
    ("storage.row.pack_cache", "Pack cache", "Cache d'empaquetage"),
    (
        "storage.row.pack_cache.desc",
        "Files gathered while packing maps. Rebuilt on the next compile.",
        "Fichiers rassemblés lors de l'empaquetage des maps. Reconstruits à la prochaine compilation.",
    ),
    ("storage.row.compile_content", "Compile content", "Contenu de compilation"),
    (
        "storage.row.compile_content.desc",
        "Content extracted for compiling. Extracted again when needed.",
        "Contenu extrait pour la compilation. Extrait de nouveau au besoin.",
    ),
    ("storage.row.builds", "Compile output", "Sorties de compilation"),
    (
        "storage.row.builds.desc",
        "Compiled maps and intermediate files of the compiler. Rebuilt on the next compile.",
        "Maps compilées et fichiers intermédiaires du compilateur. Reconstruits à la prochaine compilation.",
    ),
    ("storage.row.compile_temp", "Compile temporary files", "Fichiers temporaires de compilation"),
    (
        "storage.row.compile_temp.desc",
        "Working copies of maps for quick compiles, progressive compiles and portal checks.",
        "Copies de travail des maps pour les compilations rapides, progressives et les contrôles de portails.",
    ),
    ("storage.row.compile_history", "Compile history", "Historique de compilation"),
    (
        "storage.row.compile_history.desc",
        "The list of past compiles shown in the compile history.",
        "La liste des compilations passées affichée dans l'historique de compilation.",
    ),
    ("storage.row.release_builds", "Release builds", "Builds de publication"),
    (
        "storage.row.release_builds.desc",
        "Working files of the release packer.",
        "Fichiers de travail de l'outil de publication.",
    ),
    ("storage.row.decompiled", "Decompiled maps and extracted assets", "Maps décompilées et assets extraits"),
    (
        "storage.row.decompiled.desc",
        "Maps decompiled for the lab and models extracted from games. Extracted again when needed.",
        "Maps décompilées pour le labo et modèles extraits des jeux. Extraits de nouveau au besoin.",
    ),
    ("storage.row.sniffer", "BSP Sniffer results", "Résultats du BSP Sniffer"),
    (
        "storage.row.sniffer.desc",
        "The VMF, BSP, content and report of each BSP Sniffer run (default output folder). Deleting it removes the maps you sniffed.",
        "Le VMF, le BSP, le contenu et le rapport de chaque opération du BSP Sniffer (dossier de sortie par défaut). La supprimer efface les maps capturées.",
    ),
    ("storage.row.content_index", "Content index", "Index du contenu"),
    (
        "storage.row.content_index.desc",
        "The saved index of game content that makes startup fast. Rebuilt by the next content scan.",
        "L'index enregistré du contenu des jeux qui accélère le démarrage. Reconstruit par la prochaine analyse du contenu.",
    ),
    ("storage.row.updates", "Update downloads", "Téléchargements de mises à jour"),
    (
        "storage.row.updates.desc",
        "Downloaded update packages and their working folders in the system temporary folder.",
        "Paquets de mise à jour téléchargés et leurs dossiers de travail dans le dossier temporaire du système.",
    ),
    ("storage.row.temp", "Temporary files", "Fichiers temporaires"),
    (
        "storage.row.temp.desc",
        "Hammer Pro files in the system temporary folder (lab, model import, lighting).",
        "Fichiers de Hammer Pro dans le dossier temporaire du système (labo, import de modèles, éclairage).",
    ),
    ("storage.row.other_cache", "Other cached data", "Autres données en cache"),
    (
        "storage.row.other_cache.desc",
        "Anything else Hammer Pro keeps in its cache folder.",
        "Tout ce que Hammer Pro garde d'autre dans son dossier de cache.",
    ),
    ("storage.risk.recovery", "Recovery data", "Données de récupération"),
    ("storage.risk.user", "Your data", "Vos données"),
    ("storage.title", "Storage", "Stockage"),
    ("storage.heading", "Disk usage of Hammer Pro", "Espace disque utilisé par Hammer Pro"),
    (
        "storage.intro",
        "What Hammer Pro keeps on your disk, largest first. Only Hammer Pro's own folders are listed and cleaned: your maps and game folders are never touched.",
        "Ce que Hammer Pro garde sur votre disque, du plus gros au plus petit. Seuls les dossiers propres à Hammer Pro sont listés et nettoyés : vos maps et les dossiers des jeux ne sont jamais touchés.",
    ),
    ("storage.total", "Total used: {0} in {1} files", "Total utilisé : {0} dans {1} fichiers"),
    ("storage.refresh", "Refresh", "Actualiser"),
    ("storage.open_folder", "Open folder", "Ouvrir le dossier"),
    ("storage.delete", "Delete", "Supprimer"),
    ("storage.delete_selected", "Delete selected ({0}, {1})", "Supprimer la sélection ({0}, {1})"),
    ("storage.delete_selected_none", "Delete selected", "Supprimer la sélection"),
    ("storage.select_all", "Select all", "Tout sélectionner"),
    ("storage.select_none", "Select none", "Tout désélectionner"),
    (
        "storage.scanning",
        "Scanning: {0} of {1} locations, {2} files...",
        "Analyse : {0} emplacements sur {1}, {2} fichiers...",
    ),
    ("storage.deleting", "Deleting: {0} of {1} locations...", "Suppression : {0} emplacements sur {1}..."),
    ("storage.empty", "Empty", "Vide"),
    ("storage.files", "Files: {0}", "Fichiers : {0}"),
    ("storage.in_use_count", "{0} in use, skipped", "{0} en cours d'utilisation, ignorés"),
    ("storage.confirm.title", "Delete from disk?", "Supprimer du disque ?"),
    (
        "storage.confirm.lead",
        "This will permanently delete the following from Hammer Pro's folders. It cannot be undone.",
        "Ceci supprimera définitivement ce qui suit dans les dossiers de Hammer Pro. C'est irréversible.",
    ),
    ("storage.confirm.row", "{0} ({1})", "{0} ({1})"),
    ("storage.confirm.total", "Space freed: {0}", "Espace libéré : {0}"),
    (
        "storage.confirm.warn_user",
        "Warning: this includes your own data (settings, prefabs or saved presets). It cannot be recovered.",
        "Attention : cela inclut vos propres données (réglages, prefabs ou préréglages). Elles ne pourront pas être récupérées.",
    ),
    (
        "storage.confirm.warn_recovery",
        "Warning: this includes recovery data. Unsaved work of a crash or an interrupted session can no longer be restored.",
        "Attention : cela inclut des données de récupération. Le travail non enregistré après un plantage ne pourra plus être restauré.",
    ),
    ("storage.confirm.yes", "Delete", "Supprimer"),
    ("storage.confirm.cancel", "Cancel", "Annuler"),
    (
        "storage.compile_busy",
        "A compile is running: compile related items cannot be deleted now.",
        "Une compilation est en cours : les éléments liés à la compilation ne peuvent pas être supprimés maintenant.",
    ),
    ("storage.result.title", "Last cleanup", "Dernier nettoyage"),
    ("storage.result.freed", "Freed {0} ({1} items deleted).", "{0} libérés ({1} éléments supprimés)."),
    ("storage.result.errors", "{0} items could not be deleted:", "{0} éléments n'ont pas pu être supprimés :"),
    ("storage.result.dismiss", "Hide", "Masquer"),
    ("storage.more_paths", "{0} (+{1} more)", "{0} (+{1} autres)"),
];
