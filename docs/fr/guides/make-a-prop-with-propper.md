[Documentation](../README.md) > Guides > Créer un prop avec Propper | [English](../../en/guides/make-a-prop-with-propper.md) | [Русский](../../ru/guides/make-a-prop-with-propper.md)

# Créer un prop avec Propper

Propper transforme des brushes (et des displacements) en modèle `.mdl`, avec niveaux de détail et collision optionnelle, placé en `prop_static` ou en `prop_physics` mobile à l'endroit des brushes. Ce guide crée un prop, puis montre le chemin inverse (un prop transformé en brushes).

## Avant de commencer

- Le dossier du jeu est configuré (**Outils > Options...**, page **Jeu**). Propper compile le modèle avec le `studiomdl` du jeu, trouvé dans son dossier `bin`, ou avec le compilateur de modèles choisi dans **Paramètres > Compilation** (NekoMDL par exemple).
- Enregistrer la map donne un nom de modèle par défaut ; vous pouvez le modifier dans la fenêtre de création.
- Les exécutions automatisées (`--script`) ne compilent jamais de modèle, car `studiomdl` écrit dans le jeu.

## Étape 1 : construire la forme

1. Construisez l'objet en brushes : outil bloc, outil de découpe, outil vertex, et displacements pour les parties organiques.
2. Texturez toutes les faces visibles. Les faces avec une texture d'outil (`tools/toolsnodraw`...) sont exclues du modèle visible, mais gardent leur collision. Dans un brush de displacement, seule la face déformée est gardée, comme en jeu.
3. Restez raisonnable sur la taille : tout devient un seul modèle.

## Étape 2 : vérifier les options

Ouvrez **Outils > Placement et props > Options des props (Propper)...**. Les LOD sont partagés avec l'import de modèles (FBX, OBJ, glTF), les options physiques s'appliquent à **Convertir les brushes en prop** :

| Option | Effet | Par défaut |
|---|---|---|
| **Niveaux de détail (LOD)** | Versions simplifiées du modèle que le jeu affiche de loin (moins de triangles, même apparence). | activé |
| **Niveaux** | Niveaux sous le modèle complet, 3 à 6 (les gros modèles en ont un ou deux de plus). | 4 |
| **Chacun garde** | Part des triangles du niveau précédent que garde chaque niveau, 0.2 à 0.9 (les petits modèles sont réduits plus doucement). | 0.5 |

La fenêtre rappelle aussi comment le modèle est construit :

- Collision : chaque brush reste une pièce convexe ; les displacements ont des dalles qui suivent leur surface.
- Les matériaux de mélange (WorldVertexTransition) des displacements sont cuits dans une texture du modèle.

## Étape 3 : convertir

1. Sélectionnez les brushes (et displacements) de l'objet.
2. Choisissez **Outils > Placement et props > Convertir les brushes en prop**.
3. Dans la fenêtre de création, choisissez le chemin/nom du modèle et le dossier exporté. Les onglets Collision, Éclairage et limites et Matériaux permettent de régler les pièces de collision, `$illumposition`, `$bbox`, `$surfaceprop` et `$cdmaterials`. Ajustez leurs poignées dans l’aperçu 3D intégré, choisissez la physique/`$staticprop` et les LOD, puis cliquez sur **Créer le prop**.
4. La console affiche la préparation (textures de mélange, niveaux de détail, collision), puis la compilation `studiomdl`.
5. Après compilation et vérification, les brushes sont remplacés par le `prop_static`, `prop_dynamic` ou `prop_physics` choisi, en une étape d’annulation : `Ctrl+Z` les restaure. Si la map a changé pendant la compilation, les fichiers sont exportés mais les brushes sont conservés.

Dans l’onglet Collision, choisissez **Aucune collision**, **Boîte** ou **Précise**. Boîte produit la collision solide la plus simple ; Précise conserve les pièces des brushes/displacements et donne accès aux réglages avancés d’enveloppe et d’édition manuelle.

Le dossier de contenu exporté contient `models/<chemin choisi>.mdl` et les matériaux convertis/cuits sous `materials/<cdmaterials>/`. Les matériaux déjà en `VertexLitGeneric` gardent leurs chemins d’origine par défaut ; décochez leur réutilisation dans l’onglet Matériaux pour les copier également. Le dossier est monté automatiquement dans l’éditeur. Pour utiliser un dossier externe en jeu, installez ou montez ce contenu et les matériaux source réutilisés, ou embarquez-les avec **Construire et enregistrer en contenu** (voir [Rassembler le contenu pour une publication](collect-content-for-release.md)).

## Le chemin inverse : du prop aux brushes

Pour retoucher un prop sous forme de brushes (le vôtre ou n'importe quel modèle du jeu) :

1. Sélectionnez un `prop_static`, `prop_dynamic` ou `prop_physics` avec un modèle.
2. Choisissez **Outils > Placement et props > Convertir le prop en brushes...**. La fenêtre affiche le modèle, son nombre de triangles de rendu et son modèle de collision.
3. Choisissez les options et cliquez sur **Convertir** (`Entrée`) :

| Option | Choix | Par défaut |
|---|---|---|
| **Forme depuis** | **Automatique** : le modèle de collision quand le modèle en a un (les brushes exactes d'un prop fait avec Hammer Pro), sinon le maillage de rendu. **Modèle de collision** : une brush par pièce convexe du modèle de collision. **Maillage de rendu (pièces convexes)** : des brushes construites depuis les parties convexes du maillage visible, proches du modèle mais approximatives sur les formes courbes ou creuses. | Automatique |
| **Mettre les brushes dans** | `func_detail` ou **Le monde**. | `func_detail` |
| **Garder le prop** | Garde le prop à côté des nouveaux brushes. | désactivé |

Les faces reprennent les matériaux et le placage de texture du modèle. La conversion est une seule étape d'annulation. Les modèles trop détaillés sont refusés (trop de triangles ou de pièces).

## FAQ

**"studiomdl est introuvable dans le dossier bin du jeu".** Vérifiez le dossier de Garry's Mod dans la page **Jeu** des paramètres ; `studiomdl.exe` est fourni avec le jeu dans `bin`.

**Le prop paraît flou de loin.** C'est un niveau de détail. Augmentez **Chacun garde** ou baissez **Niveaux**, ou désactivez **Niveaux de détail (LOD)**, puis reconvertissez.

**Puis-je récupérer mes brushes plus tard ?** Oui : `Ctrl+Z` juste après, ou **Convertir le prop en brushes...** avec **Forme depuis** sur **Automatique** : un prop fait par Propper rend ses brushes exactes.

## Pages liées

- [Propper](../features/propper.md)
- [Import de modèles](../features/model-import.md)
- [Navigateur de modèles](../features/model-browser.md)
- [Outil displacement](../features/displacement-tool.md)
- [Rassembler le contenu pour une publication](collect-content-for-release.md)

---
[Retour à l'index de la documentation](../README.md)
