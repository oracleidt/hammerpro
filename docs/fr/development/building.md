[Documentation](../README.md) > Développement > Compiler depuis les sources | [English](../../en/development/building.md) | [Русский](../../ru/development/building.md)

# Compiler depuis les sources

Hammer Pro est un workspace Rust composé de petites crates. Cette page explique comment compiler l'éditeur, le lancer et exécuter les tests.

## Prérequis

- **Rust 1.95 ou plus récent** (stable), installé avec `rustup`. Le workspace utilise l'édition 2024.
- Sous Windows, la chaîne MSVC (celle que `rustup` installe par défaut sous Windows). La compilation intègre elle-même l'icône de l'application : aucun compilateur de ressources n'est nécessaire.
- Garry's Mod est facultatif : beaucoup de tests d'intégration l'utilisent quand il est présent et se désactivent sinon.

Le compilateur de maps fait partie du workspace : les outils de Valve ne sont nécessaires ni pour compiler Hammer Pro ni pour compiler des maps.

## Compiler l'éditeur

```bash
git clone <URL du dépôt>
cd HammerPro
cargo build --release -p hammerpro
```

L'exécutable est `target/release/hammerpro.exe` (`target/release/hammerpro` sous Linux). `hammerpro` est le membre par défaut du workspace, donc `cargo build --release` seul produit la même chose.

Pour le lancer depuis les sources :

```bash
cargo run                       # lance l'éditeur (profil dev)
cargo run -- chemin/vers/map.vmf  # ouvre une map
cargo run --release             # vitesse release
```

La compilation enregistre sa date et, dans un clone git, le commit, affichés tous deux dans **Aide > À propos de Hammer Pro**. `SOURCE_DATE_EPOCH` est respecté pour des compilations reproductibles.

## Profils

| Profil | Commande | Usage | Réglages |
|---|---|---|---|
| `dev` | `cargo build`, `cargo run`, `cargo test` | Développement courant. Votre code en `opt-level = 1`, toutes les dépendances en `opt-level = 3`, pour que l'éditeur reste utilisable. Tables de lignes seulement (suffisantes pour les piles d'appels), pas de cache incrémental. | `opt-level = 1`, `debug = "line-tables-only"`, `incremental = false` |
| `release` | `cargo build --release` | Les versions que vous distribuez. | `lto = "thin"`, `codegen-units = 1`, sans informations de débogage |
| `fast` | `cargo build --profile fast`, `cargo test --profile fast` | Compilations d'itération : vitesse release à l'exécution, bien plus rapide et plus léger à compiler que `release`. Sortie dans `target/fast/`. | Hérite de `release`, `lto = false`, `codegen-units = 16`, `incremental = true` |

## Tests

```bash
cargo test --workspace
```

- Les tests qui utilisent de vraies données de jeu cherchent Garry's Mod et se désactivent quand il n'est pas installé. Définissez `HAMMERPRO_GMOD_DIR` vers le dossier de Garry's Mod (celui qui contient `garrysmod` et `bin`) s'il n'est pas à l'endroit attendu par les tests. Les tests GMA lisent les éléments du Workshop dans `HAMMERPRO_WORKSHOP_DIR`.
- La table de traductions de chaque crate est vérifiée par ses tests (traductions manquantes).
- Avant une pull request, lancez les mêmes vérifications que la CI :

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Dossiers de compilation

Un dossier de compilation complet pèse plusieurs gigaoctets. Pour le placer sur un autre disque, définissez `CARGO_TARGET_DIR` :

```bat
set CARGO_TARGET_DIR=E:\builds\hammerpro
cargo build --release -p hammerpro
```

L'exécutable se trouve alors dans `E:\builds\hammerpro\release\`. Chaque profil a son propre dossier (`debug`, `release`, `fast`), donc passer de l'un à l'autre ne recompile pas tout.

## Dépendance corrigée

`vendor/wgpu-hal` est wgpu-hal 30.0.1 avec une seule modification, utilisée par `[patch.crates-io]` dans le `Cargo.toml` de l'espace de travail : sous Windows avec Vulkan, l'attente du retour d'une image de la chaîne d'échange depuis l'écran dure jusqu'à 30 secondes au lieu d'une, car une attente plus longue était traitée comme un périphérique perdu (une carte graphique occupée, un autre programme ou une image longue fermaient l'éditeur). Lors d'une mise à jour de wgpu, vérifiez si la nouvelle version transforme encore ce délai en périphérique perdu ; sinon, supprimez le dossier et le correctif.

## Linux et macOS

Les versions publiées ne sont compilées que pour Windows. Sous Linux et macOS, vous compilez Hammer Pro vous-même avec les commandes ci-dessus et vous le mettez à jour de la même façon : `git pull`, puis une nouvelle compilation. La fenêtre de mise à jour le dit quand une nouvelle version sort au lieu de proposer de l'installer.


Le workspace se compile aussi sous Linux (la CI fait ses vérifications sous Windows et Linux). La fenêtre fonctionne sous Wayland comme sous X11. Les chemins suivent les dossiers habituels : paramètres dans `~/.config/HammerPro`, journaux et sauvegardes automatiques dans `~/.local/share/HammerPro`, caches dans `~/.cache/HammerPro`.

## FAQ

**Quelle crate compiler pour obtenir l'éditeur ?** `hammerpro` (`crates/hammerpro`), le membre par défaut.

**La compilation est lente.** Utilisez le profil `fast` pendant que vous itérez, et `--release` seulement pour les versions que vous partagez.

**Les tests sont longs ou échouent faute de fichiers du jeu.** Les tests qui ont besoin des fichiers du jeu se désactivent quand ils manquent ; pointez `HAMMERPRO_GMOD_DIR` vers votre installation pour les exécuter.

## Pages liées

- [Architecture](architecture.md)
- [Contribuer](contributing.md)
- [Étendre Hammer Pro](extending.md)
- [Installation](../getting-started/installation.md)

---
[Retour à l'index de la documentation](../README.md)
