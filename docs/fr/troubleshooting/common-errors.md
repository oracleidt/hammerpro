[Documentation](../README.md) > Dépannage > Erreurs courantes | [English](../../en/troubleshooting/common-errors.md)

# Erreurs courantes

Les problèmes que les mappeurs rencontrent le plus souvent, ce qu'ils signifient et comment les corriger. Dans le journal de compilation, les erreurs et avertissements sont cliquables : un clic amène au brush, à la face, à l'entité ou à la position concernés, et la fenêtre de compilation explique les messages connus. Voir [Limites et journaux de compilation](../compiler/limits-and-logs.md).

## La map fuit

**Ce que vous voyez :** le journal de compilation signale une fuite, la barre d'état affiche un badge **Fuite**, et une ligne (le chemin de fuite) est tracée dans les vues, d'une entité vers l'extérieur.

**Ce que cela signifie :** une entité peut atteindre le vide par un trou dans les brushes du monde. La visibilité ne peut pas être calculée, et la map s'affiche et s'éclaire mal.

**Comment corriger :**

1. Suivez le chemin de fuite de l'entité jusqu'au trou. Il se charge tout seul après une compilation qui fuit (**Map > Charger le pointfile de fuite...** en charge un à la main).
2. Bouchez le trou avec un brush **du monde**. Les `func_detail` et les entités brush ne ferment pas la map.
3. Vérifiez à nouveau sans compiler : **Map > Vérifier les fuites**. Pendant l'édition, la vérification instantanée (activée par défaut) surveille les fuites et affiche le même badge.

Causes typiques : un jour entre deux murs, un brush passé en `func_detail` sur l'extérieur de la map, une entité posée hors des murs, une skybox qui ne ferme pas. Voir [Fuites et pointfiles](../features/leaks-pointfiles.md).

## Textures en damier violet et noir en jeu

Un damier en jeu signifie que le jeu ne trouve pas un matériau.

| Cause | Correction |
|---|---|
| Le matériau n'existe ni dans le jeu ni dans vos dossiers personnalisés. | **Map > Vérifier les problèmes** liste les « matériau manquant » ; l'onglet **Ressources de la map** montre ce qui manque. Remplacez le matériau (**Outils > Rechercher et remplacer des textures...**) ou ajoutez son dossier dans **Paramètres > Jeu**. |
| Un matériau personnalisé n'a pas été empaqueté. Par défaut, le contenu personnalisé n'est empaqueté dans la map que pour les compilations **Final** et les publications ; les compilations de test le copient à la place dans l'addon de développement `garrysmod/addons/hammerpro_dev`. | Après la première copie dans l'addon de développement, redémarrez le jeu une fois. Pour une map que vous partagez, utilisez **Fichier > Construire et enregistrer en contenu...** ou réglez **Empaqueter le contenu personnalisé** sur **Toujours** dans la fenêtre de compilation. |
| Le matériau vient d'un jeu monté par `mount.cfg`, d'un addon du Workshop ou de contenu téléchargé depuis un serveur. | Les joueurs qui n'ont pas ce contenu voient aussi des damiers. Livrez les fichiers avec la map, ou remplacez-les par du contenu du jeu de base. Voir [Rassembler le contenu pour une publication](../guides/collect-content-for-release.md). |

Les entités et les modèles ont le même souci : « modèle manquant » (le prop est retiré de la map) et « son manquant » sont aussi listés par Vérifier les problèmes.

## Limites du moteur

**Ce que vous voyez :** « A hard engine limit was exceeded » (limite stricte du moteur dépassée) dans le journal, ou la jauge des limites de la barre d'état proche de son maximum. **Map > Informations sur la map** affiche chaque limite avec la quantité utilisée.

| Limite | Que faire |
|---|---|
| Brushes | Fusionnez des brushes, transformez les éléments décoratifs en props, supprimez ceux qui sont cachés. |
| Plans | Gardez les brushes sur la grille et simplifiez la géométrie hors grille ou pivotée. |
| Portails | Passez les petits brushes ou les brushes détaillés en `func_detail`. |
| Modèles de brush (4096, le monde compris) | Transformez les entités brush qui ne bougent pas en brushes du monde ou en `func_detail`. |
| Lightmaps | Augmentez l'échelle de lightmap des grandes faces ou des faces jamais vues (32 ou plus), mettez du nodraw là où les joueurs ne regardent jamais. |
| Overlays (8192) | Retirez-en, ou regroupez les décals en moins d'overlays. Un overlay doit couvrir moins de 64 faces. |
| Lumières nommées (96) | Donnez le même nom aux lumières qui s'allument ensemble, ou retirez le nom des lumières qui ne changent jamais. |
| Brushes par feuille | Passez les petits brushes de la zone en `func_detail`. |

### « Exceeded max t-junctions fixup limit » (primindices)

Le compilateur corrige les jonctions en T (là où l'arête d'une face rencontre le milieu d'une autre arête) avec de petits triangles en plus, stockés comme primitives aux indices sur 16 bits : 65 536 indices au maximum (`MAX_MAP_PRIMINDICES`). Au-delà, le compilateur écrit cet avertissement avec les totaux, et les faces restantes gardent leur simple polygone, ce qui peut faire apparaître de fines fissures ou des scintillements à leurs bords. La compilation réussit quand même. Pour repasser sous la limite, réduisez le nombre de faces : transformez le brushwork détaillé en props, retirez les faces jamais vues (nodraw), et gardez la géométrie sur la grille.

Les limites varient selon les jeux et les versions de Source ; **Paramètres > Santé de la map > Limites de :** choisit le profil auquel les jauges se comparent. Voir [Vérifier les problèmes](../features/check-problems.md) et [Limites et journaux de compilation](../compiler/limits-and-logs.md).

## Autres erreurs de compilation

| Message (sens) | Correction |
|---|---|
| Une entité est dans un brush plein | Déplacez-la dans un espace libre : sinon l'éclairage et la sky camera ne fonctionnent pas. |
| Un brush est trop petit ou trop fin | Supprimez-le ou donnez-lui au moins 1 unité d'épaisseur sur la grille. |
| Un brush a une face d'aire nulle | Supprimez-le ou refaites-le avec l'outil bloc. |
| Un brush a des faces aux contenus différents | Utilisez un seul type de matériau (eau, clip, nodraw...) sur toutes les faces du brush. |
| La texture d'une face est projetée le long de la face elle-même | Réinitialisez l'alignement de la face ; Vérifier les problèmes le corrige. |
| Un displacement ne peut pas faire partie d'une entité brush | Renvoyez le brush du displacement au monde. |
| Un modèle ne peut pas être un `prop_static` | Utilisez `prop_dynamic` ou `prop_physics` pour ce modèle. |
| Un prop statique est dans le vide | Ramenez-le à l'intérieur du monde. |
| Le fichier d'un `func_instance` est introuvable | Vérifiez son chemin (relatif à la map ou au dossier maps). |
| Le compilateur manque de mémoire | Fermez d'autres programmes, baissez le nombre de threads, ou découpez les très grosses maps. |
| Le compilateur ne connaît pas cette option | Vérifiez l'orthographe des arguments supplémentaires dans les réglages de compilation. |

La plupart de ces problèmes sont aussi détectés avant la compilation par **Map > Vérifier les problèmes** (`Alt+P`), qui sait en corriger plusieurs pour vous.

## La compilation semble bloquée dans l'éclairage

L'étape d'éclairage écrit une ligne de progression au moins toutes les 30 secondes (`Lighting 41%: direct lighting 60% (stage 2 of 7), 27 s elapsed, about 17 s left`), et la barre de progression de l'étape affiche l'étape, son pourcentage et le temps restant. Si la barre avance, la compilation travaille : le temps restant dit combien il lui faut.

| Ce que vous voyez | Que faire |
|---|---|
| Un **Éclairage direct** long | Sa durée grandit avec les luxels, le nombre de lumières que voit chaque face et le ciel sur les grandes zones extérieures. Augmentez l'échelle de lightmap des grandes faces ou des faces invisibles (les statistiques les listent), ou compilez en Normal ou Brouillon pendant que vous itérez. |
| Une étape **Props statiques** longue | **Éclairer les props statiques (par sommet)** (`-StaticPropLighting`, inclus dans Final) éclaire chaque sommet de chaque prop. Désactivez-le pendant l'itération, ou réduisez `-staticpropsamplescale`. |
| Des **Rayons des rebonds** ou une **Lumière ambiante des leafs** longs | `-bounce 1` et `-fastambient` les réduisent. |
| `Warning: the lighting stage "..." has not advanced for ...` | L'étape travaille encore sur quelque chose de grand (une face immense ou un prop très détaillé). L'avertissement dit ce qui rend cette étape lente. S'il se répète longtemps, annulez et signalez-le avec le journal ([Signaler un bug](reporting-bugs.md)). |

Le journal de l'étape nomme aussi les étapes avant qu'elles commencent (`Lighting stages: ...`), vous savez donc ce qui reste.

## Le jeu est introuvable

| Message | Correction |
|---|---|
| « Garry's Mod est introuvable : choisissez son dossier dans Paramètres > Jeu » | Indiquez le dossier de Garry's Mod (**Détecter** ou **Parcourir...**), voir [Configuration du jeu](../getting-started/game-setup.md). |
| « Le jeu de cette map n'est pas installé » | La map vise un autre jeu : installez-le, choisissez son dossier dans **Paramètres > Jeu**, ou changez le **Jeu cible** dans la fenêtre de compilation. |
| « L'exécutable du jeu est introuvable » | Vérifiez le dossier du jeu dans **Paramètres > Jeu** (pour un jeu autre que Garry's Mod, l'exécutable trouvé est indiqué sous le jeu). |
| « Enregistrez et compilez d'abord la map » / « *nom*.bsp n'est pas dans le dossier maps du jeu » | Enregistrez la map, puis compilez-la avec **Copier dans le dossier maps** activé avant **Lancer la map dans le jeu**. |

## Pages liées

- [Fuites et pointfiles](../features/leaks-pointfiles.md)
- [Vérifier les problèmes](../features/check-problems.md)
- [Limites et journaux de compilation](../compiler/limits-and-logs.md)
- [Ressources de la map](../features/map-assets.md)
- [FAQ](faq.md)

---
[Retour à l'index de la documentation](../README.md)
