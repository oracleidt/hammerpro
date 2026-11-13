[Documentation](../README.md) > Dépannage > Lien direct | [English](../../en/troubleshooting/live-link.md) | [Русский](../../ru/troubleshooting/live-link.md)

# Dépannage du lien direct

Quand le [lien direct avec Garry's Mod](../features/live-link.md) ne se comporte pas comme prévu, ouvrez d'abord **Outils > Intégrations > État du lien direct...** : les lignes **État**, **Demande une compilation**, **Non envoyé** et **Erreurs** disent en général ce qui ne va pas. Les assistants IA lisent la même chose avec l'outil MCP `live_link_status`.

## Le panneau reste sur « En attente du jeu »

1. **La map tourne dans Garry's Mod.** Le lien parle à une map lancée, pas au menu principal. Lancez-la avec **Fichier > Compiler > Lancer la map dans le jeu**.
2. **La même map.** Le jeu doit faire tourner la map du même nom que le VMF ouvert (`gm_mamap.vmf` et `gm_mamap.bsp`). Sinon l'état est *Une autre map est lancée*.
3. **Le jeu n'est pas en pause.** En solo, le jeu s'arrête tant que le menu Échap ou la console est ouvert, et le lien avec lui. Fermez le menu.
4. **Le dossier du jeu.** **Paramètres > Jeu** doit désigner le Garry's Mod que vous lancez (le panneau et `live_link_status` affichent `game_folder`). Le lien écrit dans `garrysmod/data/hammerpro_livelink` de ce dossier.
5. **Un jeu lancé avant le lien.** Le lien s'y charge tout seul 3 secondes après le démarrage (le journal dit *Lien direct : l'addon a été chargé dans le jeu lancé*). Si **Charger le lien dans un jeu déjà lancé** est désactivé, ou si la fenêtre du jeu n'a pas été trouvée, relancez la map depuis Hammer Pro (**Lancer la map dans le jeu**) ou redémarrez le jeu : l'addon est maintenant dans le dossier addons et se charge avec le jeu.
6. **Des erreurs Lua dans le jeu.** Ouvrez la console du jeu (`~`, avec `-console` dans les options de lancement) : une erreur citant `hammerpro_livelink` signifie que l'addon n'a pas pu tourner. Utilisez **Retirer l'addon**, puis **Démarrer** pour écrire une copie neuve.

## « Connexion perdue »

Le jeu n'a pas répondu depuis plus de 3 secondes : il a été fermé, il a planté, il est en pause, ou une nouvelle map se charge. Le lien se reconnecte tout seul quand le jeu répond de nouveau ; si la map a été rechargée, il retrouve les entités et renvoie les modifications.

## Une modification ne s'affiche pas dans le jeu

- **Regardez Demande une compilation.** Les brushes, les displacements, `light_environment`, les sorties supprimées et les changements de classe ne peuvent pas changer en jeu. Compilez.
- **Regardez Non envoyé.** *not_in_game* signifie que l'entité de la map n'a pas été trouvée dans le jeu (le compilateur l'a retirée, ou elle a été renommée et déplacée depuis la compilation). *brush_entities* signifie qu'une entité brush a été ajoutée : il faut compiler. Les classes qui exécutent des commandes ne sont jamais envoyées.
- **Regardez Erreurs.** *entity not found in game* signifie que l'entité a été retirée dans le jeu (par la logique du jeu, ou cassée). **Renvoyer** renvoie tout.
- **Entités de la map trouvées dans le jeu** est faible : le jeu fait tourner une compilation plus ancienne que celle attendue. Compilez et rechargez, ou appuyez sur **Le jeu correspond à la map** si le jeu fait déjà tourner la map telle qu'elle est ouverte.
- **Certaines valeurs n'agissent qu'à l'apparition.** Le moteur ne lit quelques valeurs que quand l'entité apparaît (par exemple le modèle de certaines entités brush ou quelques flags). Supprimez l'entité puis annulez pour la recréer avec les nouvelles valeurs, ou compilez.

## Le prop statique apparaît deux fois

C'est normal : le moteur ne peut pas masquer un `prop_static` compilé. Le lien dessine le nouveau comme modèle côté client ; l'ancien disparaît à la prochaine compilation.

## L'aperçu de lumière ne ressemble pas à la lumière compilée

L'aperçu est une lumière dynamique (`light`) ou une texture projetée (`light_spot`), une approximation marquée *Aperçu Hammer Pro*. L'éclairage précalculé, les rebonds et les ombres de la map compilée ne changent qu'à la compilation. Désactivez **Prévisualiser les lumières dans le jeu** si vous ne voulez pas d'aperçus.

## Le joueur bouge tout seul

**Le joueur suit la caméra 3D** est actif : chaque mouvement de caméra téléporte le joueur. Désactivez-le dans **Outils > Intégrations** ou dans le panneau. **Ramener le joueur** le renvoie là où il était avant la première téléportation.

## La latence est élevée

En général 30 à 100 ms. Des valeurs plus hautes viennent d'un jeu très lent (peu d'images par seconde), d'un disque très lent pour le dossier de Garry's Mod, ou d'un antivirus qui analyse `garrysmod/data`. Exclure `garrysmod/data/hammerpro_livelink` de l'analyse en temps réel aide.

## Tout retirer

**Outils > Intégrations > Retirer l'addon du lien direct de Garry's Mod** (ou `live_link_stop` avec `remove_addon` par MCP) supprime `garrysmod/addons/hammerpro_livelink` et les fichiers de `garrysmod/data/hammerpro_livelink`. Pour le retirer à la main, fermez Garry's Mod et supprimez ces deux dossiers. Rien d'autre n'est installé.
