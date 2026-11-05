[Documentation](../README.md) > Développement > Contribuer | [English](../../en/development/contributing.md) | [Русский](../../ru/development/contributing.md)

# Contribuer

Les contributions sont les bienvenues : code, traductions, rapports de bug et documentation. Cette page résume les règles ; la référence est [CONTRIBUTING.md](../../../CONTRIBUTING.md) à la racine du dépôt (en anglais).

## Mise en place

- Installez Rust stable 1.95 ou plus récent avec `rustup`.
- Garry's Mod est facultatif, mais beaucoup de tests d'intégration l'utilisent quand il est présent. Définissez `HAMMERPRO_GMOD_DIR` s'il est installé à un endroit personnalisé.
- `cargo run` lance l'éditeur, `cargo run -- chemin/vers/map.vmf` ouvre une map. Voir [Compiler depuis les sources](building.md).

## Avant d'ouvrir une pull request

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

La CI lance les mêmes commandes sous Windows et Linux.

## Règles de code

| Règle | Pourquoi |
|---|---|
| Lisez d'abord [ARCHITECTURE.md](../../ARCHITECTURE.md) ; gardez des crates ciblées et des dépendances dans un seul sens. | Voir [Architecture](architecture.md). |
| Les bibliothèques ne paniquent jamais sur une entrée invalide : elles renvoient des erreurs. | Les maps et fichiers de jeu viennent de partout. |
| Gardez le thread de l'interface libre : les traitements lourds tournent dans des threads de fond et rendent compte par des canaux. | L'éditeur doit rester fluide sur des maps énormes. |
| Chaque texte visible passe par `hp_app::i18n`, avec un texte anglais et un texte français dans la table `TR` de la crate. Les tests de la crate repèrent les traductions manquantes. | L'interface change de langue à chaud. |
| Aucun tiret cadratin ni demi-cadratin nulle part (code, commentaires, textes de l'interface, documentation). La CI les refuse. | Style du projet. |
| Prenez chaque couleur de l'interface dans les jetons de `hp_app::theme`. La couleur est réservée au contenu (vues, miniatures, couleurs des entités). | L'interface est en noir et blanc. |
| Écrivez « Hammer Pro » dans tout ce que lisent les utilisateurs ; les noms de crates, de l'exécutable, de dossiers et de fichiers restent `hammerpro`. | Un seul nom de produit. |
| Ajoutez des tests pour tout nouveau comportement. Les tests qui ont besoin des fichiers du jeu se désactivent quand ils manquent. | La CI n'a aucun jeu installé. |
| Les actions et les paramètres sont accessibles au serveur MCP sans code supplémentaire (`run_action`, `set_setting`) ; une opération qui mérite son propre outil MCP en reçoit un, et la liste des outils de MCP.md est régénérée. | Les applications d'IA pilotent tout l'éditeur (voir [MCP.md](../../MCP.md) et la section « MCP tools » de [EXTENDING.md](../../EXTENDING.md)). |

Les nouvelles fonctionnalités se branchent sur les registres de `hp-app` (actions, paramètres, menus, pages de paramètres, barre d'état, surimpressions, données annexes par map) : voir [Étendre Hammer Pro](extending.md) et [EXTENDING.md](../../EXTENDING.md).

## Traductions

Chaque crate d'interface a une table `TR` d'entrées `(clé, anglais, français)`. Pour améliorer une traduction, modifiez le texte dans cette table. Pour ajouter une langue, ajoutez une variante à `hp_app::i18n::Lang` et une colonne aux tables.

## Signaler des bugs

Indiquez la version de Hammer Pro, votre GPU et si possible le VMF qui montre le problème, avec le rapport de plantage et le fichier journal. Voir [Signaler un bug](../troubleshooting/reporting-bugs.md).

## Licence

Hammer Pro est distribué sous licence MIT (voir [LICENSE](../../../LICENSE)).

## Pages liées

- [CONTRIBUTING.md](../../../CONTRIBUTING.md)
- [Compiler depuis les sources](building.md)
- [Architecture](architecture.md)
- [Étendre Hammer Pro](extending.md)

---
[Retour à l'index de la documentation](../README.md)
