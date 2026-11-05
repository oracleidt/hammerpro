[Documentation](../README.md) > Fonctionnalités > Propper | [English](../../en/features/propper.md)

# Propper

Propper transforme des brushes et des displacements sélectionnés en modèle compilé, avec collision optionnelle, niveaux de détail et matériaux de mélange cuits, et les remplace par un `prop_static` ou un `prop_physics` mobile. Unpropper fait l'inverse : il reconstruit un prop en brushes modifiables.

## Ouvrir Propper

| Commande | Où | Ce qu'il faut |
|---|---|---|
| **Convertir les brushes en prop...** | Menu **Outils**, et un bouton de la palette d'outils | Des brushes sélectionnés (du monde ou d'entités). Grisé sinon : « Sélectionnez les brushes à transformer en prop ». |
| **Convertir le prop en brushes...** | Menu **Outils**, et un bouton de la palette d'outils | Une entité `prop_*` sélectionnée dont le `model` est un `.mdl`. Grisé sinon : « Sélectionnez un prop à retransformer en brushes ». |
| **Options des props (Propper)...** | Menu **Outils** | Rien. |

Aucune n'a de raccourci par défaut (attribuez-en un dans **Paramètres > Clavier**).

## Convertir les brushes en prop

### Fenêtre de création

**Convertir les brushes en prop...** ouvre d'abord une petite fenêtre de confirmation. Choisissez les options puis **Créer le prop** (ou Entrée). **Annuler** ou Échap ferme la fenêtre avant toute création de fichiers ou remplacement de brushes.

| Option | Effet | Défaut |
|---|---|---|
| **Prop physique mobile** | Crée un modèle et une entité `prop_physics` ; désactivé, crée un `prop_static` fixe. | Désactivé |
| **Générer un modèle de collision** | Génère les pièces convexes de collision. Désactivé, crée un décor statique sans collision. Obligatoire pour un prop physique. | Activé |
| **Masse (kg)** | Masse physique compilée, de 0,1 à 50000 kg ; visible pour les props physiques. | 10 |
| **Niveaux de détail (LOD)** | Active les LOD et règle leur nombre et leur réduction. | Activé, 4 niveaux, 50 % |

La fenêtre **Options des props (Propper)...** contient les mêmes valeurs par défaut. L'import de modèles partage les LOD ; il conserve ses propres réglages physiques. Les options de création sont enregistrées dans `lab.propper_physics`, `lab.propper_collision` et `lab.propper_mass`.

Via MCP, réglez ces clés et `lab.model_lod` avec `set_setting`, sélectionnez les brushes puis utilisez `run_action` avec `brushes_to_prop`. Confirmez avec `ui_key` Entrée ou `ui_click` sur Créer le prop.

### Avant de commencer

- Garry's Mod doit être configuré (**Paramètres > Jeu**) ou détecté, et son studiomdl doit exister (`bin/win64/studiomdl.exe`, sinon `bin/studiomdl.exe`) : Propper compile avec. Des messages vous préviennent quand le dossier du jeu ou studiomdl manque.
- Sélectionnez les brushes et displacements qui forment le prop. Seules leurs faces visibles comptent ; une sélection sans aucune donne « La sélection n'a aucune face visible pour faire un prop. »

### Ce qui se passe

1. La géométrie est collectée. Les faces gardent leur matériau et leur alignement de texture (ombrage plat, comme des brushes). Les displacements gardent leurs normales lissées ; comme en jeu, seule la face déplacée d'un brush displacement existe.
2. La propriété de surface du modèle est celle qui domine parmi les matériaux utilisés.
3. Un modèle ne peut pas utiliser les matériaux de mélange (`WorldVertexTransition`) des displacements : chaque displacement concerné reçoit un rectangle d'un atlas de texture (4096 texels de large au plus) où les deux textures sont mélangées par l'alpha peint, comme le fait le shader (avec `$blendmodulatetexture`, la texture `$detail`, et les bump maps mélangées en une normal map). Un matériau `VertexLitGeneric` affiche le résultat. Le reste du modèle garde ses matériaux.
4. Les niveaux de détail sont faits selon les réglages partagés (voir plus bas).
5. Lorsque la collision est activée, le modèle de collision garde chaque brush comme une pièce convexe, faces d'outils comprises, pour que le prop entre en collision comme les brushes. Les displacements aussi : leur surface est découpée en dalles qui pendent 8 unités sous elle, pour que les joueurs marchent et que les props reposent sur la surface exacte.
6. Les fichiers source SMD et QC sont écrits dans le dossier `propsrc` du cache de Hammer Pro, les textures cuites dans `materials/models/<modèle>/` du dossier du jeu, puis studiomdl compile `models/hammerpro/<nom de la map>_<code>.mdl` dans le jeu (une map pas encore enregistrée s'appelle `map`). La console affiche sa sortie.
7. Le contenu est remonté et les brushes sont remplacés par un `prop_static` ou un `prop_physics` au centre de leur boîte englobante, selon le type choisi (une seule étape d'annulation). Les entités brush laissées vides sont supprimées.

La notification et la console donnent le résultat : niveaux de détail avec leur nombre de triangles, pièces de collision, propriété de surface, et la taille de l'atlas cuit s'il y en a un.

## Options des props (Propper)

Cette fenêtre regroupe les options de création ci-dessus. L'[import de modèles](model-import.md) partage les LOD dans `[lab.model_lod]` ; le type physique et la collision de cette fenêtre s'appliquent à la conversion de brushes.

| Option | Effet | Défaut |
|---|---|---|
| **Niveaux de détail (LOD)** | Crée des versions simplifiées du modèle que le jeu affiche de loin (moins de triangles, même apparence). | Activé |
| **Niveaux** | Niveaux sous le modèle complet, de 3 à 6. Les gros modèles en ont un ou deux de plus. | 4 |
| **Chacun garde** | Part des triangles du niveau précédent que garde chaque niveau, de 20 à 90 %. Les petits modèles sont réduits plus doucement. | 50 % |

Les niveaux basculent à des distances qui doublent à chaque niveau. La simplification préserve les coutures de texture, les bords entre matériaux, les trous et la silhouette : les zones plates sont réduites en premier. La fenêtre rappelle aussi comment la collision et les matériaux de mélange sont traités. **Fermer** ou `Échap` la ferme.

## Convertir le prop en brushes (Unpropper)

La fenêtre **Convertir le prop en brushes** travaille sur le premier prop sélectionné (`prop_static`, `prop_dynamic`, `prop_physics` ou tout `prop_*` avec un modèle `.mdl`). Elle lit le modèle dans le contenu du jeu, avec les bodygroups de sa clé `body`, les matériaux de son `skin`, et son origine, ses angles et son échelle. La fenêtre affiche le chemin du modèle, son nombre de triangles de rendu, et s'il a un modèle de collision (« Modèle de collision : N pièces convexes (une brush chacune). » ou « Pas de modèle de collision : les brushes sont construites depuis le maillage de rendu. »).

| Option | Effet | Défaut |
|---|---|---|
| **Forme depuis** - **Automatique** | Le modèle de collision quand le modèle en a un (les brushes exactes d'un prop fait avec Hammer Pro), sinon le maillage de rendu. | Choisi |
| **Forme depuis** - **Modèle de collision** | Une brush par pièce convexe du modèle de collision. Refusé pour un modèle qui n'en a pas. | |
| **Forme depuis** - **Maillage de rendu (pièces convexes)** | Des brushes construites depuis les parties convexes du maillage visible : proches du modèle, approximatives sur les formes courbes ou creuses. | |
| **Mettre les brushes dans** | **func_detail** (un nouveau), ou **Le monde** (groupées quand il y a plusieurs brushes). | func_detail |
| **Garder le prop** | Laisse le prop dans la map à côté des brushes. | Désactivé |

Ces choix sont enregistrés (`unprop_shape`, `unprop_detail`, `unprop_keep_prop` dans `[lab]`). **Convertir** (`Entrée`) lance la conversion, **Annuler** (`Échap`) ferme la fenêtre.

Chaque face de brush reprend le matériau et l'alignement de texture des triangles du modèle posés dessus ; les faces que le modèle ne dessine pas (faces cachées, faces nodraw et d'outils des brushes d'origine) reçoivent `tools/toolsnodraw`. Les nouveaux brushes sont sélectionnés, le prop est supprimé sauf si vous le gardez, le tout en une seule étape d'annulation. Une notification indique le nombre de brushes, de faces texturées et de faces nodraw, combien de brushes ne font qu'approcher le modèle, et combien de parties étaient trop petites ou trop plates pour devenir une brush.

Limites : 20000 triangles de rendu au plus pour reconstruire, et 1024 brushes au plus par modèle.

## Astuces

- Un prop fait par Propper avec collision activée redonne exactement ses brushes d'origine avec **Automatique** ou **Modèle de collision** : le modèle de collision de Propper est fait de ces brushes.
- Embarquez vos props dans un pack de contenu avec **Construire et enregistrer en contenu** : les modèles sont dans `models/hammerpro/` du jeu (voir [Construire et publier le contenu](build-content-release.md)).
- Via MCP, `run_action brushes_to_prop` et l'outil `prop_to_brushes` font la même chose, et les options de LOD sont les paramètres `lab.model_lod.enabled`, `lab.model_lod.levels` et `lab.model_lod.reduction` (voir la [référence des outils MCP](../../MCP.md), en anglais).

## FAQ

**La conversion écrase-t-elle les VMT source ?** Les noms des matériaux ordinaires sont conservés. Les matériaux de mélange des displacements produisent un atlas et un VMT `VertexLitGeneric` dédié sous `materials/models/hammerpro/<modèle>/blend.vmt`. Ces fichiers générés appartiennent à l'export du modèle.

**« studiomdl est introuvable dans le dossier bin du jeu. »** Propper compile avec le studiomdl de Garry's Mod : vérifiez le dossier du jeu dans **Paramètres > Jeu**. L'[import de modèles](model-import.md), lui, n'en a pas besoin.

**Le prop a compilé mais les brushes sont toujours là.** La console montre la sortie de studiomdl ; « Le modèle a compilé mais les brushes n'ont pas pu être remplacés » donne la raison.

**Unpropper répond « Aucune partie du modèle n'est assez grande pour faire une brush ».** Le modèle est fait de parties fines ou minuscules ; essayez un autre choix de **Forme depuis**.

**Est-ce que ça marche dans les lancements automatisés (`--script`) ?** Non : studiomdl écrit dans le jeu, donc les lancements automatisés ne compilent pas de modèles.

## Pages liées

- [Faire un prop avec Propper](../guides/make-a-prop-with-propper.md)
- [Import de modèles](model-import.md)
- [Navigateur de modèles](model-browser.md)
- [Outil displacement](displacement-tool.md)
- [Opérations sur les brushes](brush-operations.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
