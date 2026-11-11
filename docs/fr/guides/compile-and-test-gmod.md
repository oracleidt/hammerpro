[Documentation](../README.md) > Guides > Compiler et tester dans Garry's Mod | [English](../../en/guides/compile-and-test-gmod.md) | [Русский](../../ru/guides/compile-and-test-gmod.md)

# Compiler et tester dans Garry's Mod

Ce guide emmène une map de l'éditeur jusqu'à Garry's Mod : la compiler avec le compilateur intégré de Hammer Pro, lancer le jeu dessus, puis itérer rapidement. Pas besoin de VBSP, VVIS ni VRAD : Hammer Pro compile la map lui-même.

## Avant de commencer

- Garry's Mod est installé via Steam. Hammer Pro le trouve tout seul au démarrage. Sinon, ouvrez **Outils > Options...** (`Ctrl+,`), page **Jeu**, et renseignez le **Dossier de Garry's Mod**.
- La map contient au moins un `info_player_start` et elle est étanche (aucun trou vers le vide).
- Le nom du fichier de la map ne contient ni espace ni point (`gm_ma_map.vmf`, pas `gm ma.map.vmf`) : le jeu ne sait pas charger ces noms et la compilation les refuse.

## Étape 1 : compiler avec F9

1. Appuyez sur `F9` (**Fichier > Compiler > Compiler maintenant**). La map est compilée tout de suite avec les derniers réglages, sans ouvrir de fenêtre. La barre d'état affiche la progression.
2. La compilation travaille sur une copie de la map écrite dans `.hammerpro_build\` à côté d'elle : votre `.vmf` n'est jamais enregistré ni modifié par une compilation.
3. Avec les réglages par défaut, le BSP est copié dans `garrysmod\maps` et Garry's Mod démarre sur la map avec `-console -dev`.

Si la map n'a jamais été enregistrée, Hammer Pro vous demande d'abord où l'enregistrer.

## Étape 2 : la fenêtre Compiler et lancer

Appuyez sur `Ctrl+F9` (**Fichier > Compiler > Compiler et lancer...**) pour choisir les réglages. La fenêtre s'ouvre à part (**Fenêtre séparée**).

1. **Jeu cible** : le jeu pour lequel la map est faite. Gardez Garry's Mod (voir [Porter une map vers un autre jeu](port-to-another-game.md) pour les autres).
2. **Préréglage** :
   - **Brouillon** : itération rapide. S'arrête sur les fuites, visibilité rapide, éclairage d'aperçu (un rebond, une seule plage dynamique), pas d'intégration de contenu. Jouable en quelques secondes.
   - **Normal** : qualité standard, visibilité complète et éclairage par défaut. C'est le réglage par défaut.
   - **Final** : qualité de publication, visibilité complète et éclairage final dans les deux plages dynamiques. Lent.
   - **Personnalisé** : Normal plus vos arguments supplémentaires (voir **Avancé**).
3. **Étapes** : **Géométrie**, **Visibilité** et **Éclairage**, chacune avec son mode. Les modes "seulement les entités" et "seulement les props" de la géométrie mettent à jour en quelques secondes le BSP de la dernière compilation, en gardant sa visibilité et son éclairage.
4. **Plage dynamique** : **LDR**, **HDR** ou **LDR + HDR**. Final calcule toujours les deux.
5. **Après** : **Copier dans le dossier maps**, **Lancer le jeu** (et **Via Steam**), **Charger le chemin de fuite en cas de fuite**, **Arguments du jeu**.
6. Cliquez sur **Compiler**. L'onglet **Journal** suit chaque étape ; les erreurs et avertissements sont cliquables et vous emmènent au brush, à la face, à l'entité ou à l'endroit concerné.

Toutes les options sont décrites dans [Fenêtre de compilation](../compiler/compile-window.md) et [Options de compilation](../compiler/compile-options.md).

## Étape 3 : tester en jeu

Dans la section **Jeu** de la fenêtre :

- **Démarrer le jeu :** **après la compilation** (comme Hammer) ou **au début de la compilation** (le jeu démarre pendant que le compilateur tourne, puis charge la map).
- **Réutiliser le jeu lancé** : le test suivant charge la map dans le jeu déjà ouvert au lieu de le relancer. La barre d'état affiche **Jeu prêt** quand Garry's Mod tourne.
- **Démarrage rapide** : des options qui accélèrent le démarrage du jeu pour un test (sauter la vidéo d'intro, fenêtré, sans bordure, taille de fenêtre, console, messages développeur, **Partie locale privée**). Chacune ajoute ou retire son option dans les arguments du jeu.
- **Ignorer le Workshop et les addons inutilisés** : quand la map n'en utilise rien, le jeu ne les monte pas et démarre plus vite (les outils et armes des addons manquent alors en jeu).

Pour apparaître en jeu à l'endroit de votre caméra d'éditeur, installez l'addon pont de Hammer Pro : **Outils > Options...**, page **Jeu**, section **Tests en jeu**, **Installer**, puis cochez **Apparaître à la caméra de l'éditeur**. Redémarrez le jeu une fois après l'installation. Le pont n'agit que dans les parties locales. Il active aussi **Indiquer le temps de chargement de la map**.

## Étape 4 : itérer vite

- Gardez **Recompilation intelligente** activée (par défaut) : les compilations Brouillon et Normal ne relancent que ce qui a changé depuis la dernière compilation (entités seules, éclairage seul, props seuls, ou tout).
- Utilisez **Brouillon** pendant la construction, **Normal** pour vérifier le rendu, **Final** pour publier.
- **Fichier > Compiler > Recharger la map dans le jeu** recharge la map dans le jeu déjà lancé.
- **Jouer tout de suite, finir l'éclairage ensuite** : compile un Brouillon, lance le jeu, puis compile la qualité choisie en arrière-plan. Quand vous quittez la map en jeu, le BSP entièrement éclairé remplace la version rapide.
- Contenu personnalisé (matériaux, modèles, sons, particules) : les compilations Brouillon et Normal ne l'intègrent pas au BSP. Avec **Addon de développement** activé (par défaut), il est copié dans `garrysmod\addons\hammerpro_dev` pour que la map s'affiche correctement en jeu. Redémarrez le jeu une fois après la première copie.

## Si la map fuit

Avec **S'arrêter quand la map fuit** (par défaut), la compilation s'arrête et indique **La map fuit**. Le chemin de fuite est chargé aussitôt (**Charger le chemin de fuite en cas de fuite**) : une ligne qui va d'une entité jusqu'au vide en passant par le trou. Suivez-la dans les vues, bouchez le trou et recompilez. Vous pouvez aussi vérifier sans compiler avec **Map > Vérifier les fuites**. Voir [Fuites et pointfiles](../features/leaks-pointfiles.md).

## FAQ

**Le jeu ne se lance pas.** Vérifiez **Lancer le jeu** dans la fenêtre et le jeu trouvé dans la page **Jeu** des paramètres. Les exécutions automatisées (`--script`) ne lancent jamais le jeu.

**Mes textures personnalisées sont violettes et noires en jeu.** Gardez **Addon de développement** activé et redémarrez le jeu une fois, ou utilisez le préréglage **Final** ou **Construire et enregistrer en contenu**, qui les intègrent à la map.

**La compilation me demande d'enregistrer.** Seulement pour une map jamais enregistrée. L'enregistrement avant compilation est désactivé par défaut : la compilation n'en a jamais besoin.

## Pages liées

- [Fenêtre de compilation](../compiler/compile-window.md)
- [Options de compilation](../compiler/compile-options.md)
- [Lancement du jeu](../features/game-launch.md)
- [Fuites et pointfiles](../features/leaks-pointfiles.md)
- [Erreurs courantes](../troubleshooting/common-errors.md)
- [Rassembler le contenu pour une publication](collect-content-for-release.md)

---
[Retour à l'index de la documentation](../README.md)
