[Documentation](../README.md) > Guides > Porter une map vers un autre jeu | [English](../../en/guides/port-to-another-game.md) | [Русский](../../ru/guides/port-to-another-game.md)

# Porter une map vers un autre jeu

Hammer Pro est pensé d'abord pour Garry's Mod, mais son compilateur écrit aussi des maps pour d'autres jeux Source : Half-Life 2 et ses épisodes, Counter-Strike: Source, Team Fortress 2, Portal, Portal 2, Left 4 Dead 2 et d'autres (la liste complète est dans [Jeux cibles](../compiler/target-games.md)). Ce guide fait passer une map de Garry's Mod à un autre jeu.

## Comment fonctionne le jeu d'une map

- Le **jeu par défaut** se règle dans **Outils > Options...**, page **Jeu**, champ **Jeu**. Toute map sans jeu propre l'utilise.
- Chaque map peut avoir **son propre jeu**, choisi dans la fenêtre de compilation (**Jeu cible**). Il est gardé avec la map, dans son fichier annexe (section `game`), pas dans le VMF : Hammer et les autres éditeurs ne sont pas concernés.
- Quand le jeu actif change, son contenu est monté et ses définitions d'entités (FGD) sont rechargées. La compilation écrit son format de BSP, vérifie ses limites et lance ses passes d'éclairage.

## Étape 1 : installer le jeu

Installez le jeu cible via Steam. Hammer Pro lit les bibliothèques Steam et le trouve tout seul. Dans les listes de jeux, les jeux installés viennent en premier ; les autres sont marqués **(non installé)**. Si le jeu est installé hors des dossiers de Steam, choisissez son dossier dans la page **Jeu** des paramètres (**Dossier du jeu**, avec **Rechercher à nouveau** pour relire les bibliothèques Steam).

## Étape 2 : faire une copie de la map

Utilisez **Fichier > Enregistrer comme nouvelle version...** ou enregistrez une copie sous un autre nom (par exemple `de_mamap.vmf` à partir de `gm_mamap.vmf`) pour garder intacte la version Garry's Mod. Le jeu d'une map est lié à son fichier, la copie peut donc viser un autre jeu.

## Étape 3 : choisir le jeu cible

1. Ouvrez la copie et appuyez sur `Ctrl+F9` (**Fichier > Compiler > Compiler et lancer...**).
2. Dans la ligne **Jeu cible** en haut, choisissez le jeu. Hammer Pro monte son contenu et charge ses FGD.
3. La ligne en dessous indique ce que signifie compiler pour lui : sa version de BSP et de props statiques, **Limites de Valve** (Garry's Mod a des limites relevées), et **Non installé** si le jeu manque (la map se compile alors avec le contenu monté actuellement, et elle n'est ni copiée dans un jeu ni lancée).

## Étape 4 : corriger ce qui n'existe pas dans le nouveau jeu

- **Contenu** : les textures et modèles de Garry's Mod ou de vos addons n'existent peut-être pas dans le nouveau jeu. Ouvrez **Map > Ressources de la map**, filtrez sur **Manquants**, puis remplacez-les avec **Outils > Textures > Rechercher et remplacer des textures...** (`Ctrl+Maj+R`) ou en modifiant les entités.
- **Entités** : certaines classes n'existent que dans Garry's Mod. Lancez **Map > Vérifier les problèmes** (`Alt+P`) : les classes inconnues et les cibles manquantes sont listées avec leurs objets.
- **Fonctions propres à Garry's Mod** : les addons, le Workshop, `mount.cfg`, l'interrupteur de skybox en Lua (`point_hammerpro_skybox`), les tests en jeu avec l'addon pont et la publication sur le Workshop sont désactivés pour les autres jeux. **Construire et enregistrer en contenu** ne concerne que Garry's Mod : compilez les autres jeux depuis la fenêtre de compilation (`F9`).
- **Limites** : la map a désormais les limites de Valve. Vérifiez **Map > Informations sur la map** et le journal de compilation.

## Étape 5 : compiler et tester

Appuyez sur `F9`. Si le jeu est installé, le BSP est copié dans son dossier `maps` et le jeu démarre dessus, comme pour Garry's Mod (voir [Compiler et tester dans Garry's Mod](compile-and-test-gmod.md)).

## Revenir en arrière

Choisissez de nouveau Garry's Mod dans **Jeu cible**. Une map dont le jeu est le jeu par défaut n'enregistre rien : choisir le jeu par défaut efface le choix propre à la map.

## FAQ

**Le jeu voulu n'est pas dans la liste.** La liste ne montre que les jeux que le compilateur prend en charge. Les jeux Source 2 et quelques variantes du moteur (par exemple Counter-Strike 2, Dota 2, Vampire: The Masquerade - Bloodlines) sont connus mais pas pris en charge.

**Ma map compile mais les textures manquent dans l'autre jeu.** La map utilise encore du contenu de Garry's Mod. Livrez les fichiers avec la map ou remplacez-les par le contenu du jeu (voir l'étape 4).

## Pages liées

- [Jeux cibles](../compiler/target-games.md)
- [Configuration du jeu](../getting-started/game-setup.md)
- [Fenêtre de compilation](../compiler/compile-window.md)
- [Vérifier les problèmes](../features/check-problems.md)
- [Ressources de la map](../features/map-assets.md)
- [Fichiers et dossiers](../reference/files-and-folders.md)

---
[Retour à l'index de la documentation](../README.md)
