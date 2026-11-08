[Documentation](../README.md) > Fonctionnalités > Soundscapes | [English](../../en/features/soundscapes.md) | [Русский](../../ru/features/soundscapes.md)

# Soundscapes

L'éditeur de soundscapes écrit le fichier de soundscapes de votre map : les ambiances sonores que jouent les entités `env_soundscape` (vent, oiseaux, machines, bruits lointains aléatoires). Il modifie `soundscapes_<map>.txt` sous forme de liste de soundscapes, chacun avec ses règles, vous fait choisir les sons parmi les fichiers sonores du contenu du jeu et vous permet de les écouter.

## Ouverture

**Map > Soundscapes...** ouvre la fenêtre **Soundscapes de la map** (pas de raccourci par défaut). Elle reste ouverte pendant que vous travaillez ; fermez-la avec son bouton de fermeture.

La map doit d'abord être enregistrée : l'éditeur travaille sur le fichier `soundscapes_<map>.txt` placé à côté du fichier de la map. Pour une map jamais enregistrée, la fenêtre affiche **Enregistrez d'abord la map : son fichier de soundscapes est écrit à côté d'elle.**

Le haut de la fenêtre montre le chemin du fichier, **(non enregistré)** s'il y a des modifications en attente, et un rappel : **Le jeu charge scripts/soundscapes_<map>.txt : empaquetez ce fichier avec la map. Les entités env_soundscape les jouent.**

## Boutons

| Bouton | Rôle |
|---|---|
| **Enregistrer** | Écrit le fichier. Actif quand quelque chose a changé. L'écriture est sûre (fichier temporaire, puis renommage par-dessus l'ancien) et n'a lieu que quand vous cliquez **Enregistrer**. |
| **Recharger** | Relit le fichier sur le disque et abandonne les modifications non enregistrées. |

Si le fichier est illisible (par exemple s'il dépasse 4 Mo), la fenêtre affiche **Le fichier est illisible (enregistrement désactivé)** : impossible de l'écraser par erreur.

## La liste des soundscapes

À gauche :

- **Nom du nouveau soundscape** et **+** (**Ajouter un soundscape**) ajoutent un soundscape avec `dsp` à `1`. Un nom ne peut pas être vide, contenir d'espace, de guillemet ou d'accolade, ni exister déjà dans le fichier.
- Cliquez un soundscape pour le modifier.
- Un soundscape dont le nom est déjà utilisé par un soundscape du contenu du jeu monté est signalé **nom utilisé par le jeu**. Les noms de soundscapes sont globaux dans le moteur : renommez-le.

## Modifier un soundscape

À droite, pour le soundscape sélectionné :

- **Supprimer** retire le soundscape.
- **Effet DSP** règle sa clé `dsp`.
- **+ playlooping**, **+ playrandom** et **+ playsoundscape** ajoutent une règle.
- Chaque règle a un **×** (**Supprimer cette règle**).

Tous les champs sont du texte simple. Vider un champ supprime sa clé ; les guillemets doubles ne sont pas acceptés.

### Règles

| Règle | Champs | Créée avec |
|---|---|---|
| `playlooping` (un son en boucle) | **Volume**, **Hauteur**, **Son**, **Position**, **Niveau sonore** | `volume 1`, `pitch 100`, un son vide |
| `playrandom` (des sons aléatoires à intervalles aléatoires) | **Volume**, **Hauteur**, **Délai (min,max)**, les sons de son bloc `rndwave`, **Niveau sonore** | `volume 1`, `pitch 100`, `time 10,20`, un son vide |
| `playsoundscape` (joue un autre soundscape) | **Volume**, **Soundscape** (nom de l'autre soundscape) | `volume 1`, un nom vide |

Chaque ligne **Son** a un champ texte, un bouton **♪** qui liste les fichiers sonores du contenu du jeu, un bouton **▶** qui joue le son, et **×** (**Retirer ce son**). **+ son** ajoute un son à une règle `playrandom`. Le volume d'écoute est celui de **Paramètres > Entités et navigateurs > Volume de l'écoute des sons**.

Les clés laissées vides (une règle ajoutée sans son pour l'instant) ne sont pas écrites dans le fichier, pour que le moteur ne s'en plaigne pas. Les clés et blocs que l'éditeur ne connaît pas sont conservés tels quels.

## Astuces

- Préfixez vos soundscapes avec le nom de la map (par exemple `gm_mamap.foret`) pour éviter l'avertissement **nom utilisé par le jeu**.
- Le moteur lit `scripts/soundscapes_<map>.txt` : placez ou empaquetez le fichier à cet endroit pour que le jeu l'utilise. Voir [Construire et enregistrer en contenu](build-content-release.md) et [Ressources de la map](map-assets.md).
- Choisissez les sons avec le bouton **♪** plutôt qu'en les tapant : la liste ne contient que des fichiers présents dans le contenu monté.
- Par MCP, `run_action` avec `edit_soundscapes` ouvre la fenêtre. Voir la [référence des outils MCP](../../MCP.md) (en anglais).

## FAQ

**Pourquoi la fenêtre est-elle vide avec un message ?** La map n'a jamais été enregistrée : il n'y a pas de dossier où écrire le fichier de soundscapes. Enregistrez la map puis rouvrez l'éditeur.

**Pourquoi Enregistrer est-il grisé ?** Rien n'a changé depuis le dernier enregistrement ou rechargement, ou le fichier était illisible (l'enregistrement est alors désactivé pour le protéger).

**Mon soundscape ne se joue pas en jeu.** Vérifiez qu'une entité `env_soundscape` utilise son nom, que le fichier est dans `scripts/` sous le nom `soundscapes_<map>.txt` (empaqueté avec la map), et que le nom n'est pas signalé comme utilisé par le jeu.

**Puis-je modifier les soundscapes du jeu lui-même ?** Non. L'éditeur ne modifie que le fichier de votre map. Les soundscapes du jeu sont lus seulement pour signaler les conflits de noms.

## Pages liées

- [Navigateur de sons](sound-browser.md)
- [Propriétés des entités](entity-properties.md)
- [Ressources de la map](map-assets.md)
- [Construire et enregistrer en contenu](build-content-release.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)

## Assigner un soundscape personnalisé

Pour `env_soundscape` et `env_soundscape_triggerable`, saisissez directement le nom de l’entrée du script dans **Soundscape**, par exemple `sunless_cavesound_entree`, puis validez avec Entrée. Le bouton voisin propose les choix du FGD et les noms des scripts de soundscapes montés. Les noms personnalisés sont conservés dans le VMF et prennent en charge Annuler/Rétablir.
