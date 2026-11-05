[Documentation](../README.md) > Fonctionnalités > Éditeur de sons | [English](../../en/features/sound-editor.md)

# Éditeur de sons

L'éditeur de sons ouvre un son du contenu du jeu, des addons, de la map ou de votre disque, affiche sa forme d'onde, permet de changer son volume, de le couper, de le fondre et de le faire boucler, joue le résultat, puis l'enregistre en WAV dans le dossier de votre choix. Le fichier enregistré est toujours un WAV PCM 16 bits, le format que le moteur Source lit : les fichiers MP3 et OGG s'ouvrent et sont réécrits en WAV.

## L'ouvrir

- **Outils > Éditeur de sons...** (action `sound_editor`) ouvre la fenêtre, et la ferme au second appel.
- MCP : `open_media_editor` avec `editor` à `sound` (et un `path` pour charger un son).

## Choisir un son

La liste de gauche a deux sources :

| Source | Contenu |
|---|---|
| **Sons du jeu** | Les fichiers son (WAV, MP3, OGG) du contenu du jeu monté, des addons et des archives du Workshop, le même index que le [navigateur de sons](sound-browser.md) et l'outil MCP `search_sounds`. |
| **Utilisés par la map** | Les fichiers son nommés par les valeurs des entités de la map ouverte (messages d'`ambient_generic`, sons de triggers, toute valeur se terminant par `.wav`, `.mp3` ou `.ogg`). |

Le champ de recherche garde les sons contenant tous les mots. **Ouvrir un fichier son...** choisit n'importe quel fichier WAV, MP3 ou OGG de votre disque.

## L'éditeur

La ligne sous le nom donne la fréquence d'échantillonnage, les canaux, la durée, le niveau de crête et la taille. Source lit correctement les sons en 11025, 22050 et 44100 Hz ; pour toute autre fréquence, un avertissement suggère le réglage **Fréquence d'échantillonnage**.

| Contrôle | Effet |
|---|---|
| Forme d'onde | La partie gardée est dessinée à la couleur d'accent, les parties coupées sont assombries. Glissez près de la ligne verte pour déplacer le début, près de la rouge pour déplacer la fin. La ligne jaune est la position de lecture, la bleue le début de boucle. |
| **Lire** / **Arrêter** | Joue le son modifié (en boucle si **Boucler le son** est coché). **Original** joue le son non modifié. **Volume** n'est que le volume de l'écoute. |
| **Réinitialiser** | Revient au son non modifié (en gardant les marqueurs de boucle d'origine). |
| **Volume (gain)** | Gain de -40 à +24 dB. Le petit chiffre est le niveau de crête après le gain ; la sortie est écrêtée à 0 dBFS. |
| **Normaliser à** + **Régler le gain** | Règle le gain pour que l'échantillon le plus fort de la partie gardée atteigne le niveau (-1 dB par défaut). |
| **Début**, **Fin**, **Durée** | La partie gardée, en secondes. Changer la durée déplace la fin. |
| **Fondu d'entrée**, **Fondu de sortie** | Fondus linéaires aux extrémités de la partie gardée. |
| **Boucle** | **Boucler le son** écrit les marqueurs de boucle que le moteur lit pour répéter le son : un bloc `cue ` et un bloc `smpl` avec une boucle avant du début de boucle au dernier échantillon. **dès** est le début de boucle en secondes depuis le début de la partie gardée (0 par défaut). Les sons qui portent déjà des marqueurs s'ouvrent avec la boucle activée. |
| **Canaux** | **Mixer en mono** pour un son stéréo (un son mono est positionné en 3D par les effets `snd_*`, pas un son stéréo). |
| **Fréquence d'échantillonnage** | **Conserver**, 11025, 22050 ou 44100 Hz (rééchantillonnage linéaire). |

## Enregistrer

- **Dossier** est la racine sous laquelle le fichier est écrit. Par défaut c'est le dossier au-dessus du dossier `maps` de la map enregistrée (votre addon ou projet), sinon le dossier du jeu. **Parcourir...** choisit un autre dossier.
- **Fichier** est le chemin sous ce dossier ; `sound/` est ajouté s'il manque et l'extension est toujours `.wav`. La ligne en dessous montre le chemin écrit. Par défaut c'est le chemin du son d'origine avec l'extension `.wav` : une copie modifiée de `sound/ambient/wind.mp3` dans votre addon remplace donc l'original en jeu.
- Si le fichier existe, cochez **Remplacer le fichier existant**.
- **Enregistrer en WAV** écrit le fichier et rafraîchit l'index du contenu pour que le nouveau son apparaisse dans les navigateurs.

Les fichiers écrits dans le dossier du jeu ne font pas partie d'une étape d'annulation.

## MCP

L'éditeur est entièrement accessible par les outils (voir la [référence des outils MCP](../../MCP.md), en anglais) :

| Outil | Usage |
|---|---|
| `sound_info` | Fréquence, canaux, durée, crête, marqueurs de boucle, forme d'onde. |
| `sound_edit` | L'éditeur entier en un appel : `gain_db` ou `normalize_db`, `start` avec `end` ou `length`, `fade_in`, `fade_out`, `looped`, `loop_start`, `mono`, `sample_rate`, `output`, `root`, `overwrite`, `dry_run`. |
| `sound_preview` | Joue la modification, ou arrête (`stop`). |
| `list_map_sounds` | La liste « Utilisés par la map ». |
| `search_sounds` | La liste des sons du jeu. |
| `open_media_editor` | Ouvre la fenêtre, éventuellement avec un son chargé. |

## Voir aussi

- [Navigateur de sons](sound-browser.md)
- [Soundscapes](soundscapes.md)
- [Correction des couleurs](color-correction.md)
