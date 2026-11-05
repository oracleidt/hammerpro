[Documentation](../README.md) > Fonctionnalités > Navigateur de sons | [English](../../en/features/sound-browser.md) | [Русский](../../ru/features/sound-browser.md)

# Navigateur de sons

Le navigateur de sons liste les sons du contenu du jeu monté : soit les sons du jeu définis par les soundscripts (`Streetwar.d3_c17_13_horn`), soit les fichiers sonores (`ambient/alarms/klaxon1.wav`), avec un lecteur d'écoute, un volume, la lecture automatique et l'entrée de soundscript de chaque son du jeu. Les clés de son du panneau d'entité ont aussi leur propre sélecteur rapide et peuvent vous envoyer dans le navigateur pour choisir un son.

## Ouvrir le navigateur

- **Affichage > Panneaux > Navigateur de sons** ajoute l'onglet **Sons** au panneau de droite (ou le retire). Enregistré dans `show_sound_browser` de `[browser]` (désactivé par défaut).
- Le bouton de parcours d'une clé de son dans le panneau d'entité l'ouvre aussi (voir « Choisir un son pour une clé » plus bas).
- Pas de raccourci par défaut. Avec le préréglage de touches Hammer++ (**Paramètres > Clavier**), c'est `Maj+F12`.
- Le bouton `🗗` (**Ouvrir dans une fenêtre**) sort le navigateur dans sa propre fenêtre flottante ; `⊟` (**Remettre dans le panneau**) le remet dans le panneau.

## La liste

| Contrôle | Effet |
|---|---|
| **Sons du jeu** / **Fichiers** | Ce que montre la liste : les sons du jeu des soundscripts, ou les fichiers sonores. |
| `⟳` **Recharger les soundscripts** | Relit les soundscripts (après en avoir modifié un). |
| **Rechercher des sons (!mot exclut)** | Garde les sons qui contiennent tous les mots (sans tenir compte de la casse). En mode sons du jeu, un mot peut correspondre au nom ou au fichier joué. Un mot qui commence par `!` exclut. |
| Pastilles de source **Tout**, **Jeu**, **Addons**, **Workshop** | Ne gardent que les sons d'un type de source (une pastille n'apparaît que si le contenu a ce type). |
| **Contenu de Garry's Mod uniquement** | Seulement ce que tous les joueurs de Garry's Mod possèdent. Partagé avec les navigateurs de modèles et de particules (`gmod_only` dans `[browser]`). |
| **Sons : N** | Nombre de sons listés (« Lecture des soundscripts... » pendant le chargement). |

Chaque ligne a un bouton d'écoute, le nom et, en mode sons du jeu, le premier fichier wave qu'il joue, puis un badge qui indique d'où vient le fichier (**GMod**, **jeu : ...**, **addon**, **Workshop ...**, **perso**, ou **introuvable** quand aucun fichier monté ne correspond). Le son en cours de lecture est en gras. Survolez une ligne pour voir le nom complet de la source.

Les sons du jeu viennent de `scripts/game_sounds_manifest.txt` (ses entrées `precache_file`, `preload_file` et `autocache_file`), lus en arrière-plan une fois par montage et triés par nom. Quand deux soundscripts définissent le même nom, le premier l'emporte.

## Souris et clavier

- Un clic sélectionne un son ; avec **Lire à la sélection** activé, il est joué.
- Le bouton d'écoute d'une ligne joue ce son.
- Double-clic : en mode choix, écrit le son dans la clé ; sinon le joue.
- Pointeur sur la liste : `Haut` / `Bas` déplacent la sélection (et la jouent avec **Lire à la sélection**), `Entrée` fait comme un double-clic.

## Le bas du navigateur

| Contrôle | Effet | Défaut |
|---|---|---|
| Son sélectionné | Son nom (ou « Sélectionnez un son »). | |
| **▶ Écouter** / **■ Arrêter** | Joue ou arrête le son sélectionné. | |
| Curseur de volume (0 à 100 %) | **Volume de l'écoute** de toutes les écoutes, y compris celles du panneau d'entité. `sound_volume` dans `[browser]` (0 à 1). | 80 % |
| **Lire à la sélection** | Sélectionner un son le joue. `sound_autoplay` dans `[browser]`. | Activé |
| **Soundscript...** | Pour un son du jeu : ouvre une petite fenêtre avec son entrée de soundscript (le nom du fichier en premier). Survolez le bouton pour voir le fichier de soundscript. | |
| **Copier le nom** | Copie le nom sélectionné dans le presse-papiers. | |
| **Remplacer partout (N)** | En mode choix, quand la clé contient déjà un autre son : toutes les entités dont la clé contient l'ancien son reçoivent le son sélectionné (une seule étape d'annulation), et le mode choix se termine. | |

L'écoute lit les fichiers WAV (PCM et ADPCM), MP3 et OGG du contenu monté, sous Windows uniquement ; ailleurs, le bas du navigateur indique qu'aucune écoute n'est possible. Un son du jeu joue son premier fichier wave.

## Choisir un son pour une clé

Une clé de son du panneau d'entité (par exemple `message` de `ambient_generic`) a trois boutons (voir [Propriétés des entités](entity-properties.md)) :

- `♪` ouvre un sélecteur rapide : une liste filtrable des sons du jeu (avec le fichier qu'ils jouent) suivis des fichiers sonores. Chaque élément a un bouton d'écoute. Vous pouvez aussi taper une valeur absente de la liste.
- `▶` / `■` joue ou arrête le son contenu dans la clé (grisé quand il ne peut pas être joué).
- Le bouton de parcours (« Choisir dans le navigateur de sons (double-cliquez sur un son) ») ouvre le navigateur de sons en mode choix.

En mode choix, le navigateur affiche un bandeau « Choix pour *clé* : double-cliquez sur un élément, ou sélectionnez-le puis Utiliser. » Le son déjà présent dans la clé est sélectionné (le navigateur passe sur **Fichiers** quand la valeur est un fichier `.wav`, `.mp3` ou `.ogg`). Double-cliquez un son, appuyez sur `Entrée` ou cliquez **Utiliser** pour l'écrire (une seule étape d'annulation) ; le panneau revient sur l'onglet **Entité**. **Annuler** quitte le mode choix.

## Astuces

- Désactivez **Lire à la sélection** pour parcourir une longue liste en silence avec les flèches, puis cliquez le bouton d'écoute de la ligne voulue.
- L'[éditeur de soundscapes](soundscapes.md) utilise le même sélecteur rapide de sons.
- Via MCP, `search_sounds` cherche dans les sons du contenu (voir la [référence des outils MCP](../../MCP.md), en anglais).

## FAQ

**Pourquoi la liste reste vide ?** Le contenu du jeu n'est pas chargé (« Le contenu du jeu n'est pas chargé : définissez le dossier du jeu dans les paramètres »), ou la recherche, les pastilles de source ou **Contenu de Garry's Mod uniquement** masquent tout.

**J'ai modifié un soundscript, le navigateur montre l'ancienne entrée.** Cliquez `⟳` pour relire les soundscripts.

**Pourquoi je n'entends rien ?** Seuls les fichiers WAV, MP3 et OGG du contenu monté sont lus, sous Windows. Vérifiez le curseur de volume, et que le badge n'indique pas **introuvable**.

## Pages liées

- [Propriétés des entités](entity-properties.md)
- [Soundscapes](soundscapes.md)
- [Navigateur de modèles](model-browser.md)
- [Particules](particles.md)
- [Éditeur de sons](sound-editor.md)
- [Configuration du jeu](../getting-started/game-setup.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
