[Documentation](../README.md) > Développement > Étendre Hammer Pro | [English](../../en/development/extending.md) | [Русский](../../ru/development/extending.md)

# Étendre Hammer Pro

Hammer Pro est construit à partir de paquets de fonctionnalités qui se branchent sur des registres partagés au lieu de modifier les fichiers des autres. Cette page est une carte rapide de ces points d'extension. Le guide complet du développeur, avec le code de chaque point d'accroche, est [docs/EXTENDING.md](../../EXTENDING.md) (en anglais, écrit pour les contributeurs).

## La règle

Travaillez dans vos propres fichiers. Quand un fichier hôte appartient à un autre paquet, branchez-vous par un point d'accroche. Chaque paquet a une clé utilisée dans les noms de fichiers : shell, selection (1), brush (2), workflow (3), instances (4), visibility (5), helpers (6), mesh (7), entities (8), render (9), lighting (10), placement (11), compile (12), startup (13), launch (14), content (15), health (16), lab (17) et mcp.

## Points d'extension

| Point d'extension | À quoi il sert | Où |
| --- | --- | --- |
| Actions | Toutes les commandes (menus, barre d'outils, raccourcis, palette de commandes, référence `F1`, paramètres du clavier, scripts, MCP). Un seul enum, déclaré dans un fichier par paquet avec `action_list!` : id, menu, groupe de menu, raccourci par défaut, statut. | `crates/hp-app/src/actions/<paquet>.rs` |
| Menus | Place d'une action dans un menu ou un sous-menu (`MenuGroup`), triée par section et par ordre. | `crates/hp-app/src/actions/groups.rs` |
| Dispatch | Le code qui exécute une action. | `crates/hammerpro/src/dispatch/<paquet>.rs` |
| Paramètres | Une structure par paquet, enregistrée dans sa propre table TOML, chaque champ avec une valeur par défaut. | `crates/hp-app/src/settings/<paquet>.rs` |
| Pages de paramètres | Pages de la fenêtre des paramètres. | `crates/hp-ui-shell/src/settings_dialog/<paquet>.rs` |
| Barre d'état | Segments de la barre d'état. | `crates/hp-ui-shell/src/status/` |
| Fichier annexe (sidecar) | Données propres à une map, gardées hors du VMF. | `crates/hp-app/src/sidecar.rs` |
| Calques d'overlay | Géométrie dessinée par les vues par-dessus la scène. | `crates/hp-ui-viewports/src/layers/` |
| Producteurs de scène | Objets ajoutés à la scène rendue. | voir EXTENDING.md |
| Accroches d'outils et sélection dans les vues | Outils qui réagissent aux entrées, sélection dans les vues. | voir EXTENDING.md |
| Extension de l'éditeur | Blocs VMF supplémentaires (les blocs Hammer++ `*_plus` sont conservés ainsi). | `crates/hp-editor` |
| Compilation et lancement | Étapes de la chaîne de compilation, arguments de lancement. | `crates/hp-compile`, `crates/hp-ui-mapping/src/compile/` |
| Traductions | Chaque texte de l'interface sous forme de lignes `(clé, anglais, français)` d'une table `TR`. | chaque crate |
| Lignes de stockage | Chaque dossier de cache, de sortie ou temporaire écrit par Hammer Pro, listé et nettoyé par Paramètres > Stockage et MCP. | `crates/hp-app/src/storage/locations.rs` (`TABLE`) |
| Outils MCP | Outils sémantiques du serveur MCP. Chaque action et chaque paramètre sont déjà accessibles par `run_action` et `set_setting`. | `crates/hammerpro/src/dispatch/mcp/tools_<domaine>.rs` |

## Traductions

Chaque texte visible passe par la couche i18n, avec un texte anglais et un texte français. Les clés sont préfixées par leur domaine (`shell.`, `tex.`, `disp.`...) et doivent être uniques dans toutes les crates. Le contrôle des tables refuse les textes vides, les clés en double et les caractères tiret cadratin ou demi-cadratin. Le français met une espace avant `:`, `?` et `!`. Ajouter une langue, c'est ajouter une variante à `hp_app::i18n::Lang` et une colonne aux tables.

## Apparence et couleurs

L'interface est en noir et blanc : chaque couleur de l'interface vient des jetons de `hp_app::theme`. La couleur est réservée au contenu (vues, vignettes, couleurs des entités). Les couleurs qui portent un sens dans les données (catégories de nœuds et types de broches de l'éditeur de shaders, canaux R, V et B de l'étalonnage, grille du terrain) vivent dans `crates/hp-app/src/data_colors.rs`, le seul fichier où le test du thème accepte des couleurs teintées.

## Ajouter un outil MCP

1. Décrivez l'outil dans les `SPECS` d'un fichier de domaine `crates/hammerpro/src/dispatch/mcp/tools_<domaine>.rs`.
2. Implémentez-le dans la fonction `run` de ce fichier ; chaque modification passe par une étape d'annulation nommée « MCP: <outil> ».
3. Ajoutez le libellé d'annulation `shell.undo.mcp_<outil>` dans `crates/hp-app/src/actions/mcp.rs`.
4. Régénérez la liste des outils de `docs/MCP.md` avec `HP_UPDATE_MCP_DOCS=1 cargo test -p hammerpro docs_list_every_tool`.

## Remarque

[docs/EXTENDING.md](../../EXTENDING.md) a été écrit pendant la construction des paquets : ses tableaux qualifient encore certaines actions de « Planned » (prévues). Dans la version 1.0.2, toutes les actions qui y sont listées sont disponibles (voir [Actions et raccourcis clavier](../reference/actions-shortcuts.md)).

## Pages liées

- [Architecture](architecture.md)
- [Compiler depuis les sources](building.md)
- [Contribuer](contributing.md)
- [Référence des outils MCP](../../MCP.md) (en anglais)

---
[Retour à l'index de la documentation](../README.md)
