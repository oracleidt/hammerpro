[Documentation](../README.md) > Fonctionnalités > Propper | [English](../../en/features/propper.md) | [Русский](../../ru/features/propper.md)

# Propper

Propper transforme des brushes et des displacements sélectionnés en modèle compilé, avec collision optionnelle, niveaux de détail et matériaux de mélange cuits, et les remplace par un `prop_static`, un `prop_dynamic` ou un `prop_physics` mobile. Unpropper fait l'inverse : il reconstruit un prop en brushes modifiables.

## Ouvrir Propper

| Commande | Où | Ce qu'il faut |
|---|---|---|
| **Convertir les brushes en prop...** | Menu **Outils**, et un bouton de la palette d'outils | Des brushes sélectionnés (du monde ou d'entités). Grisé sinon : « Sélectionnez les brushes à transformer en prop ». |
| **Convertir le prop en brushes...** | Menu **Outils**, et un bouton de la palette d'outils | Une entité `prop_*` sélectionnée dont le `model` est un `.mdl`. Grisé sinon : « Sélectionnez un prop à retransformer en brushes ». |
| **Options des props (Propper)...** | Menu **Outils** | Rien. |

Aucune n'a de raccourci par défaut (attribuez-en un dans **Paramètres > Clavier**).

## Convertir les brushes en prop

### Fenêtre de création

**Convertir les brushes en prop...** ouvre une fenêtre à quatre onglets avec un aperçu 3D texturé intégré. Réglez les options avant de cliquer sur **Créer le prop**. **Annuler** ou Échap ferme la fenêtre sans exporter de fichiers ni remplacer les brushes.

| Onglet | Options |
|---|---|
| **Modèle** | Chemin et nom relatifs à `models/` (extension `.mdl` facultative), dossier du contenu exporté, dossier des sources QC/SMD, physique, `$staticprop`, masse et LOD. |
| **Collision** | Activer la collision ; conserver les pièces des brushes, générer une enveloppe convexe ou une boîte, ou modifier les pièces convexes manuellement. Ajoutez/supprimez des pièces et déplacez leurs sommets avec les champs XYZ ou les poignées 3D. La collision est obligatoire pour les props physiques. |
| **Éclairage et limites** | Régler `$illumposition` avec les champs XYZ ou son marqueur. Choisir les limites automatiques ou un `$bbox` personnalisé : champs minimum/maximum, huit coins et axes colorés dans la vue 3D ; le point central déplace toute la boîte sans changer sa taille. Les coordonnées sont locales à l’origine du modèle, au centre de la sélection. |
| **Matériaux** | Définir `$cdmaterials`, relatif à `materials/`. Les VMT et leurs dépendances VTF sont copiés dans ce dossier. Choisir `$surfaceprop` dans toutes les surfaces déclarées par le jeu monté, y compris les surfaces personnalisées, ou utiliser automatiquement le matériau dominant. |

Glisser avec le clic droit fait tourner la vue, le clic milieu la déplace et la molette zoome. Les surimpressions de collision, de limites et du point d’éclairage sont indépendantes ; **Cadrer** inclut le modèle et les points modifiables. L’aperçu se met à jour pendant les modifications.

La case `$staticprop` crée un `prop_static` lorsqu’elle est cochée, un `prop_dynamic` lorsqu’elle est décochée ; elle est désactivée pour un `prop_physics` mobile. Par défaut : collision activée, masse de 10 kg, quatre LOD à 50 %. **Options des props (Propper)...** conserve les valeurs par défaut du type, de la collision, de la masse et des LOD (`lab.propper_physics`, `lab.propper_static`, `lab.propper_collision`, `lab.propper_mass`, `lab.model_lod`). Les chemins et la géométrie manuelle se règlent dans la fenêtre de création.

Via MCP, réglez les valeurs par défaut avec `set_setting`, sélectionnez les brushes puis lancez `brushes_to_prop`. Vérifiez la fenêtre et cliquez sur **Créer le prop**. Entrée valide lorsqu’aucun champ de texte n’a le focus clavier.

### Avant de commencer

- Garry's Mod doit être configuré (**Paramètres > Jeu**) ou détecté, et son studiomdl doit exister (`bin/win64/studiomdl.exe`, sinon `bin/studiomdl.exe`) : Propper compile avec. Des messages vous préviennent quand le dossier du jeu ou studiomdl manque.
- Sélectionnez les brushes et displacements qui forment le prop. Seules leurs faces visibles comptent ; une sélection sans aucune donne « La sélection n'a aucune face visible pour faire un prop. »

### Ce qui se passe

1. La géométrie est collectée. Les faces gardent leur matériau et leur alignement de texture (ombrage plat, comme des brushes). Les displacements gardent leurs normales lissées ; comme en jeu, seule la face déplacée d'un brush displacement existe.
2. La propriété de surface est celle choisie dans la liste, ou celle du matériau dominant en mode automatique.
3. Un modèle ne peut pas utiliser les matériaux de mélange (`WorldVertexTransition`) des displacements : chaque displacement concerné reçoit un rectangle d'un atlas de texture (4096 texels de large au plus) où les deux textures sont mélangées par l'alpha peint, comme le fait le shader (avec `$blendmodulatetexture`, la texture `$detail`, et les bump maps mélangées en une normal map). Un matériau `VertexLitGeneric` affiche le résultat. Le reste du modèle garde ses matériaux.
4. Les niveaux de détail sont faits selon les réglages partagés (voir plus bas).
5. La collision utilise le mode choisi. Par défaut, les pièces d’origine conservent chaque brush comme une partie convexe, faces d’outils comprises. Les displacements sont découpés en dalles descendant de 8 unités sous leur surface. Les modes enveloppe, boîte et manuel utilisent la géométrie affichée dans l’aperçu de collision.
6. Les sources QC/SMD sont écrites dans le dossier choisi. studiomdl compile dans un dossier de travail privé. Le modèle, ses données de sommets/collision et les matériaux copiés sont vérifiés puis exportés dans `models/` et `materials/` du dossier de contenu choisi. La console affiche la sortie du compilateur.
7. Le dossier exporté est monté dans l’éditeur. Les brushes sont remplacés par le type de prop choisi, en une étape d’annulation. Si la map a changé pendant la compilation, les fichiers sont exportés mais les brushes d’origine sont conservés.

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
- **Construire et enregistrer en contenu** embarque le modèle exporté et ses matériaux (voir [Construire et publier le contenu](build-content-release.md)). Un dossier extérieur au jeu est monté dans l’éditeur ; installez ou montez également ce contenu en jeu pour tester.
- Via MCP, `run_action brushes_to_prop` et l'outil `prop_to_brushes` font la même chose, et les options de LOD sont les paramètres `lab.model_lod.enabled`, `lab.model_lod.levels` et `lab.model_lod.reduction` (voir la [référence des outils MCP](../../MCP.md), en anglais).

## FAQ

**La conversion écrase-t-elle les VMT source ?** Non. Le modèle utilise des VMT et VTF séparés dans le dossier `$cdmaterials` choisi. Les octets des VTF Source existants sont conservés, avec les mipmaps, l’alpha et les images d’animation. Les shaders de brushes sont adaptés à l’éclairage des modèles ; les matériaux de mélange des displacements sont cuits.

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
