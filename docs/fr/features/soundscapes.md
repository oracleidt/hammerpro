[Documentation](../README.md) > Fonctionnalités > Soundscapes | [English](../../en/features/soundscapes.md) | [Русский](../../ru/features/soundscapes.md)

# Soundscapes

Les soundscapes sont les sons d'ambiance d'une map Source : vent, oiseaux, machines, bruits lointains au hasard. Une entité `env_soundscape` nomme un soundscape du jeu ou du fichier de la map `scripts/soundscapes_<map>.txt`, et le moteur joue le soundscape de l'entité la plus proche dont le joueur est à portée et qu'il voit.

Hammer Pro a trois outils pour eux. Aucun ne tourne tant que vous ne l'activez pas :

| Outil | Où | Ce qu'il fait |
|---|---|---|
| **Éditeur de soundscapes** | **Map > Soundscapes...** (action `edit_soundscapes`) | Liste les soundscapes du jeu et de la map, les joue, modifie et enregistre le fichier de soundscapes de la map, attribue un soundscape à l'`env_soundscape` sélectionné. |
| **Vue des soundscapes** | **Affichage > Aperçu de l'environnement > Afficher les soundscapes** (action `toggle_soundscape_view`, désactivée par défaut) | Dessine les entités soundscape, leur rayon, leurs cibles de position et leurs noms, les rayons des `ambient_generic`, et colore le sol avec le soundscape que le joueur y entendrait. |
| **Écoute dans la vue joueur** | **Affichage > Aperçu de l'environnement > Entendre les soundscapes dans la vue joueur** (action `toggle_soundscape_hearing`, désactivée par défaut), ou `K` dans la vue joueur | Joue le soundscape actif et les sons `ambient_generic` pendant que vous marchez dans la vue joueur, avec la distance et les changements de soundscape comme le moteur. |

**Affichage > Aperçu de l'environnement > Options des soundscapes** contient le curseur de volume, **Muet**, **Zones actives** et **Sons ambient_generic**.

## L'éditeur de soundscapes

**Map > Soundscapes...** ouvre la fenêtre **Éditeur de soundscapes** ; la même entrée la ferme.

### Le fichier

Le haut de la fenêtre indique le fichier où les soundscapes de la map sont enregistrés : `scripts/soundscapes_<map>.txt` sous le dossier au-dessus du dossier `maps` de la map (votre dossier d'addon ou de projet), ou sous le dossier du jeu quand la map n'est pas dans un dossier `maps`. C'est le fichier que le moteur charge pour la map : il fonctionne en jeu tel quel, et [Construire et enregistrer en contenu](build-content-release.md) l'empaquette avec la map.

- **Dossier...** choisit un autre dossier de jeu ou d'addon ; **Dossier par défaut** revient en arrière.
- **(non enregistré)** s'affiche quand il y a des modifications. Elles sont gardées tant que Hammer Pro tourne (aussi quand vous fermez la fenêtre ou changez de map), l'aperçu, l'écoute et les outils MCP les utilisent déjà, mais le fichier ne change qu'avec **Enregistrer**.
- **Enregistrer** écrit le fichier sans risque (un fichier temporaire renommé par-dessus l'ancien). Un fichier listé par `scripts/soundscapes_manifest.txt` du jeu n'est jamais écrit : l'éditeur n'écrit que le fichier de la map.
- **Recharger** relit le fichier et abandonne les modifications non enregistrées.
- La map doit d'abord être enregistrée (le fichier porte son nom).
- Les anciennes versions de Hammer Pro écrivaient `soundscapes_<map>.txt` à côté du fichier de la map, où le jeu ne le lit pas. Un tel fichier est lu tant que celui de `scripts` n'existe pas, la fenêtre le signale, et **Enregistrer** l'écrit dans `scripts/` (l'ancien fichier n'est pas touché).

### La liste

À gauche, les soundscapes du fichier de la map, puis **Soundscapes du jeu** : ceux de chaque fichier de `scripts/soundscapes_manifest.txt` dans le contenu monté. **Tous**, **Map** et **Jeu** filtrent la liste et le champ de recherche garde les noms contenant chaque mot.

- **Nom du nouveau soundscape** et **+** ajoutent un soundscape au fichier de la map (avec `dsp` 1). Les noms ne peuvent contenir ni espaces, ni guillemets, ni accolades.
- Un soundscape de la map portant le nom d'un soundscape du jeu s'affiche dans la couleur d'avertissement : les noms de soundscapes sont globaux dans le moteur, renommez-le.
- Les soundscapes du jeu sont en lecture seule. **Copier dans le fichier de la map** en copie un sous un nouveau nom (`<map>.<nom>`) pour le modifier.

### Un soundscape

À droite :

| Contrôle | Ce qu'il fait |
|---|---|
| **Aperçu** / **Arrêter** | Joue le soundscape avec ses soundscapes imbriqués et ses sons aléatoires à des moments aléatoires, sans distance, jusqu'à **Arrêter**. |
| **Utiliser sur la sélection (n)** | Règle la clé `soundscape` des entités `env_soundscape` et `env_soundscape_triggerable` sélectionnées (une étape d'annulation **Attribution de soundscape**). |
| **Utilisé par** | Les entités soundscape de la map qui le nomment ; un clic en sélectionne une. |
| Ligne de résumé | Combien de sons en boucle et aléatoires il joue avec ses soundscapes imbriqués, son DSP, les noms imbriqués qui n'existent pas, les boucles d'imbrication. |
| **Effet DSP** | L'effet de pièce `dsp` que le moteur active quand ce soundscape démarre (0 aucun, 1 pièce générique, 2 à 28 les pièces standard de Source ; le nom du préréglage est affiché). Seul le soundscape nommé par une entité le règle, pas les soundscapes imbriqués, et sans la clé l'effet précédent reste. **×** supprime la clé. |
| **DSP du joueur** | `dsp_player` : un second effet que le moteur applique par-dessus l'effet de pièce (mêmes numéros), gardé jusqu'à ce qu'un autre soundscape le change. |
| **Volume de l'effet** | `dsp_volume` : force de l'effet de pièce (1 par défaut). |
| **+ playlooping**, **+ playrandom**, **+ playsoundscape** | Ajoutent une règle. Chaque règle a **×** pour la supprimer. |

Règles :

| Règle | Champs |
|---|---|
| `playlooping` : un son en boucle | **Volume**, **Hauteur**, **Son**, **Niveau sonore**, **Position** |
| `playrandom` : des sons aléatoires à des moments aléatoires | **Délai (secondes)**, **Volume**, **Hauteur**, ses sons, **Niveau sonore**, **Position** (aussi **aléatoire autour du joueur**) |
| `playsoundscape` : joue un autre soundscape | **Soundscape** (tapé ou choisi, **Ouvrir** l'affiche), **Volume** (multiplie les volumes du soundscape imbriqué), **Décalage des positions** (`position`, ajouté à ses numéros de position), **Remplacer la position**, **Position des sons ambiants** |

- **Volume**, **Hauteur** et **Délai** prennent un nombre ou, avec **plage** coché, une plage : le moteur tire une valeur au hasard dedans à chaque fois.
- **(par défaut)** après un champ veut dire que la clé n'est pas dans le fichier et que la valeur par défaut du moteur s'applique, comme en jeu : un `playlooping` sans volume ne joue rien, un `playrandom` sans volume ne joue rien, sans hauteur est ignoré par le moteur et sans délai rejoue à chaque image. L'éditeur signale chacun de ces cas ; les nouvelles règles sont ajoutées avec des valeurs qui marchent.
- **Son** : tapez le chemin sous `sound/`, ou **♪** choisit un fichier son du contenu monté (champ de recherche dans la liste), **▶** le joue, **×** le supprime. Un son absent du contenu est signalé, de même qu'un son de `playlooping` sans marqueurs de boucle (le jeu le joue une fois ; l'aperçu le répète).
- **Niveau sonore** règle jusqu'où porte un son positionné (`SNDLVL_NORM` vaut 75 dB). Les sons ambiants l'ignorent.
- **Position** : **aucune (ambiant)** joue le son partout au même niveau. **position0** à **position7** le jouent depuis l'entité nommée par la clé `position0` à `position7` de l'`env_soundscape`, en baissant avec la distance. La vue dessine ces cibles.

Les clés et blocs que l'éditeur ne connaît pas sont conservés tels quels. Les clés vides (une règle ajoutée sans son pour l'instant) ne sont pas écrites.

### Options d'écoute

La deuxième ligne de la fenêtre contient **Afficher dans les vues**, **Zones actives**, **Entendre dans la vue joueur**, **Sons ambient_generic**, **Volume** et **Muet** (les mêmes réglages que les menus). Pendant que la vue joueur entend les soundscapes, la fenêtre indique celui qui joue et pourquoi.

## La vue des soundscapes

Avec **Afficher les soundscapes**, chaque vue dessine :

- chaque `env_soundscape`, `env_soundscape_proxy` et `env_soundscape_triggerable` avec un cercle de son `radius` (trois cercles en 3D) dans une couleur propre à son nom de soundscape, une étiquette avec le nom du soundscape et le rayon (**partout** pour un rayon de -1), **(désactivé)** pour Start Disabled, l'entité principale d'un proxy, et **(par trigger_soundscape)** pour les entités déclenchables ;
- des lignes de l'entité vers ses cibles `position0` à `position7`, étiquetées **position0** à **position7** ;
- chaque `ambient_generic` avec un cercle bleu clair de son `radius`, étiqueté avec son fichier son (**(démarre muet)** le cas échéant) ;
- chaque `trigger_soundscape` sous forme de boîte dans la couleur du soundscape de l'`env_soundscape_triggerable` qu'il nomme, une ligne vers cette entité et une étiquette (ou un avertissement quand aucun `env_soundscape_triggerable` ne porte ce nom) ;
- avec **Zones actives** (par défaut), des dalles au sol dans la couleur du soundscape que le joueur entendrait debout à cet endroit, à la hauteur de la caméra 3D (ou du joueur dans la vue joueur), selon les règles du moteur ci-dessous. Dans un `trigger_soundscape` sans autre soundscape à portée et en vue, la dalle prend le soundscape de son entité déclenchable. Les dalles grises sont hors de tout rayon : le joueur continue d'entendre le soundscape entendu avant. Les dalles se remplissent en quelques images après un changement ;
- dans les vues 3D, une étiquette en coin **Soundscape ici** avec le soundscape entendu à la caméra.

Comment le moteur choisit le soundscape (le code Source SDK 2013 d'`env_soundscape` et du système de soundscapes, reproduit exactement) :

- À chaque image du serveur, le soundscape entendu est testé en premier, puis les autres entités soundscape (`env_soundscape`, `env_soundscape_proxy` et `env_soundscape_triggerable`). Un soundscape actuel désactivé est abandonné ; les désactivés sont ignorés.
- Une entité est à portée quand son `radius` est strictement plus grand que sa distance à l'oreille du joueur, ou quand le rayon vaut exactement `-1`. Tout autre rayon négatif, un rayon de 0 et un rayon absent (0) n'atteignent personne.
- À portée, elle doit aussi voir l'oreille : une trace de l'origine de l'entité à l'oreille du joueur (les yeux : 64 unités au-dessus des pieds debout, 28 accroupi) avec le masque `MASK_SOLID_BRUSHONLY | MASK_WATER`. Les brushs solides du monde, les entités brush, les vitres, les grilles, l'eau et la collision des `prop_static` solides la bloquent ; les clips, les triggers, `func_illusionary`, les autres props et les joueurs non. Une entité dont l'origine est dans un brush solide, ou exactement sur sa surface, démarre dans le solide et ne voit jamais personne.
- Un candidat qui passe prend la place quand le soundscape actuel n'est pas passé, ou quand il est plus proche. Quand aucun ne passe, le soundscape actuel reste.
- Un `env_soundscape_proxy` joue le soundscape et les cibles de position de l'entité nommée par son `MainSoundscapeName`.
- Un `trigger_soundscape` dans lequel le joueur entre rend aussitôt actuel l'`env_soundscape_triggerable` nommé par sa clé `soundscape` (le dernier entré gagne) ; en sortir revient à un autre trigger où le joueur se trouve encore, ou à aucun soundscape. Les règles de rayon tournent toujours à chaque image : un soundscape à portée et en vue reprend la main. Quand le serveur n'a plus de soundscape, le client continue de jouer le dernier.
- Le moteur ne teste que les soundscapes dont la visibilité (PVS) atteint le cluster du joueur ; une trace dégagée dans le rayon le garantit toujours, le résultat est donc le même.

## L'écoute dans la vue joueur

Activez **Entendre les soundscapes dans la vue joueur**, entrez dans la vue joueur (F6) et marchez. Hammer Pro fait tourner le système de soundscapes client du moteur :

- Quand le soundscape change, chaque son en boucle va vers son nouveau volume d'un tiers du volume plein par seconde (`soundscape_fadetime`, 3 secondes) ; un son en boucle de même fichier et de même hauteur dans le nouveau soundscape continue à son nouveau volume. Les boucles ambiantes démarrent muettes, les boucles positionnées à 0,05.
- Les sons `playrandom` démarrent après une demi-période aléatoire, puis attendent un `time` aléatoire entre deux lectures, avec leur volume, leur hauteur et leur niveau sonore aléatoires ; `position random` les joue à 36 unités autour des yeux, comme le moteur.
- Les sons positionnés baissent avec la distance selon leur niveau sonore et passent à gauche ou à droite ; les sons ambiants jouent à leur volume partout.
- L'effet de pièce (`dsp`), `dsp_player` et `dsp_volume` du soundscape sont appliqués : une réverbération, un écho et un filtre proches de chaque préréglage standard de Source (pièces, métal, tunnels, salles, pièces claires, sous l'eau, béton, extérieur, cavernes, étrange), en fondu sur 3 secondes. Les valeurs au-delà de 28 jouent la pièce générique.
- Les volumes `trigger_soundscape` activent leur `env_soundscape_triggerable` quand vous les traversez.
- Avec **Sons ambient_generic**, les entités `ambient_generic` jouent comme au démarrage de la map : celles en boucle en continu sauf Start Silent, les autres une fois ; `radius` devient un niveau sonore comme le moteur, `health` le volume, `pitch`, Play Everywhere, Source Entity Name. Leurs entrées `PlaySound`, `StopSound`, `ToggleSound`, `Volume`, `Pitch`, `FadeIn` et `FadeOut`, et les entrées `Enable`, `Disable` et `ToggleEnabled` des entités soundscape, envoyées par la simulation des entrées et sorties de la vue joueur, sont suivies.

`K` active ou coupe l'écoute dans la vue joueur ; les compteurs indiquent le soundscape joué, les sons et le volume. Les clés `Soundmixer` ne changent que l'équilibre avec les sons du jeu (armes, voix) que l'éditeur ne joue pas. La lecture passe par le périphérique audio par défaut sous Windows ; la sortie audio est libérée dix secondes après le dernier son.

## MCP

Tout est accessible par les outils (voir la [référence des outils MCP](../../MCP.md)) :

| Outil | Usage |
|---|---|
| `list_soundscapes` | Les soundscapes du jeu et de la map et chaque entité soundscape de la map. |
| `get_soundscape` | Un soundscape avec ses règles typées, son texte KeyValues, la liste aplatie des sons et les sons trouvés. |
| `edit_soundscape` | Créer, modifier (`rules`, `add_rules`, `dsp`, `dsp_player`, `dsp_volume`, `rename`), copier (`copy_from`) ou supprimer un soundscape de la map, l'attribuer (`assign_to`), `dry_run`. |
| `preview_soundscape` | Joue un soundscape ou l'arrête ; `record_to` enregistre dans un fichier WAV ce que la sortie des soundscapes envoie au périphérique audio, `record_status` indique quand il est écrit, avec son niveau de crête et ses niveaux RMS. |
| `soundscape_at` | Le soundscape entendu en un point, chaque candidat avec distance, portée et ligne de vue, et les sons `ambient_generic` audibles. |

Actions (`run_action`) : `edit_soundscapes`, `toggle_soundscape_view`, `toggle_soundscape_hearing`. Paramètres (`set_setting`) : `lab.soundscape.view`, `lab.soundscape.areas`, `lab.soundscape.hearing`, `lab.soundscape.volume`, `lab.soundscape.mute`, `lab.soundscape.ambient_generic`.

## Astuces

- Préfixez les soundscapes de la map avec le nom de la map (`gm_mamap.foret`) pour qu'aucun soundscape du jeu ne porte le même nom.
- Donnez des sons en boucle aux règles `playlooping` (l'éditeur de sons sait ajouter des marqueurs de boucle : [Éditeur de sons](sound-editor.md)).
- Activez la vue et regardez les dalles au sol : une pièce qui entend le soundscape extérieur a souvent besoin d'un `env_soundscape` à l'intérieur, plus proche que celui de l'extérieur.

## FAQ

**Mon soundscape ne joue pas en jeu.** Vérifiez qu'un `env_soundscape` utilise son nom (la vue l'affiche), que le joueur est dans son rayon et le voit (les dalles le disent), et que le fichier est `scripts/soundscapes_<map>.txt` dans le dossier du jeu ou de l'addon, ou empaqueté avec la map.

**Pourquoi l'ancien soundscape continue-t-il dans un couloir ?** Hors de tout rayon, le moteur garde le soundscape actuel. Ajoutez un `env_soundscape` qui couvre le couloir.

**Puis-je modifier les soundscapes du jeu ?** Non, ils sont en lecture seule. Copiez-en un dans le fichier de la map et modifiez la copie.

**Un env_soundscape ne joue jamais alors que le joueur est tout près.** Son origine est sans doute dans un brush ou exactement sur le sol : la trace du moteur démarre alors dans le solide et échoue. Montez-le de quelques unités au-dessus du sol ; `soundscape_at` indique `start_solid` pour chaque candidat.

**Je n'entends rien dans la vue joueur.** L'écoute est désactivée par défaut : activez-la (ou appuyez sur `K` dans la vue joueur), vérifiez **Muet** et le volume, et que les sons existent (l'éditeur signale ceux qui manquent).

## Pages liées

- [Vue joueur](player-view.md)
- [Navigateur de sons](sound-browser.md)
- [Éditeur de sons](sound-editor.md)
- [Propriétés des entités](entity-properties.md)
- [Ressources de la map](map-assets.md)
- [Construire et enregistrer en contenu](build-content-release.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)

## Assigner un soundscape personnalisé

Pour `env_soundscape` et `env_soundscape_triggerable`, saisissez directement le nom de l'entrée du script dans **Soundscape**, par exemple `sunless_cavesound_entree`, puis validez avec Entrée. Le bouton voisin propose les choix du FGD et les noms des scripts de soundscapes montés. Les noms personnalisés sont conservés dans le VMF et prennent en charge Annuler/Rétablir. **Utiliser sur la sélection** de l'éditeur de soundscapes fait de même pour chaque entité soundscape sélectionnée.
