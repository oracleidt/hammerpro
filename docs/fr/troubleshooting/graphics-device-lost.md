[Documentation](../README.md) > Dépannage > Périphérique graphique perdu | [English](../../en/troubleshooting/graphics-device-lost.md) | [Русский](../../ru/troubleshooting/graphics-device-lost.md)

# Périphérique graphique perdu

Le périphérique graphique est perdu quand le pilote graphique réinitialise la carte (Windows le fait quand la carte ne répond plus pendant environ 2 secondes), quand la carte n'a plus de mémoire au milieu d'une image, quand le pilote plante ou quand la carte est retirée. Plus rien ne peut être dessiné avec ce périphérique.

## Ce que fait Hammer Pro

Hammer Pro continue de fonctionner et relance le graphisme tout seul :

1. Il sauvegarde chaque map modifiée (l'onglet affiché et les autres) à côté des sauvegardes automatiques, dans `%LOCALAPPDATA%\HammerPro\autosave\`, avant toute autre chose.
2. Il libère tout ce qui vivait sur le périphérique perdu, crée un nouveau périphérique graphique et une nouvelle surface de fenêtre, et lui renvoie l'interface (polices, icônes, aperçus).
3. La map, ses textures et ses modèles, les vignettes et l'aperçu d'éclairage se rechargent sur le nouveau périphérique, comme à l'ouverture d'une map. Un avis indique **La carte graphique ne répond plus** pendant ce temps, puis **Le graphisme fonctionne de nouveau**. Cela prend en général une ou deux secondes.

Rien n'est fermé et aucune modification n'est perdue : l'historique d'annulation, la sélection, la caméra et tous les onglets ouverts restent tels quels. Une image qui ne peut pas être dessinée pendant que le périphérique manque est sautée au lieu d'arrêter le programme.

Si le périphérique est perdu plus de 3 fois en deux minutes, ou si aucun nouveau périphérique ne peut être créé pendant 20 secondes, Hammer Pro revient à la méthode sûre :

1. Il écrit un rapport de plantage avec la raison et les dernières lignes du journal dans `%APPDATA%\HammerPro\crash\`.
2. Il sauvegarde chaque map modifiée.
3. Il affiche **Périphérique graphique perdu** avec la raison et demande **Relancer Hammer Pro maintenant ?**
   - **Oui** : Hammer Pro redémarre et rouvre tous vos onglets, les maps modifiées depuis leurs modifications sauvegardées. Un message indique que les maps ont été rouvertes.
   - **Non** : Hammer Pro se ferme. Au démarrage suivant, la fenêtre de plantage montre le rapport et propose **Ouvrir les modifications sauvegardées**.

Une perte du périphérique ne fait jamais perdre votre travail : au pire, vous retrouvez les modifications sauvegardées dans un nouvel onglet marqué comme modifié, à enregistrer par-dessus la map d'origine.

L'outil MCP `get_gpu_memory` indique les pertes et les reprises de la session (`device`), et `simulate_gpu_device_loss` détruit le périphérique exprès pour tester la reprise.

## Ce que Hammer Pro évite

- **Les images lentes des autres programmes.** Avec Vulkan sous Windows, une image que l'écran n'avait pas affichée en une seconde comptait comme un périphérique perdu. Hammer Pro attend maintenant plus longtemps (jusqu'à 30 secondes), donc un jeu, un calcul sur la carte graphique ou une image longue de l'éditeur ne l'arrêtent plus.
- **La mémoire insuffisante.** Hammer Pro garde son utilisation de la mémoire graphique sous un budget et libère de la mémoire avant que la carte soit pleine. Voir [Mémoire insuffisante](out-of-memory.md).
- **Les énormes envois d'un seul bloc.** Les lightmaps calculées et compilées sont envoyées à la carte par morceaux de 32 Mo (4 Mo dans le mode de mémoire graphique Faible), et l'aperçu calculé envoie ses lightmaps et leur géométrie quelques mégaoctets par image, aucun transfert n'occupe donc la carte longtemps.
- **Les petites cartes graphiques.** Sur une puce graphique intégrée et les cartes de moins de 3 Go, **Paramètres > Rendu > Mémoire graphique** réglé sur **Auto** choisit le mode **Faible**, qui prévoit un budget bien en dessous de ce que la carte contient. Voir [Mémoire insuffisante](out-of-memory.md#mode-de-memoire-graphique-faible).

## Quand cela recommence

| Ce que vous voyez | Ce qu'il faut essayer |
|---|---|
| Le périphérique est perdu à l'ouverture d'une grande map ou à l'entrée dans la vue joueur | Mettez à jour le pilote graphique. Baissez **Paramètres > Rendu > Budget de mémoire graphique**, fermez le jeu et les autres programmes 3D, fermez les onglets inutilisés. |
| Le périphérique est perdu après un moment d'édition | Regardez la ligne de mémoire graphique de **Paramètres > Rendu** : si elle est proche du budget, voir [Mémoire insuffisante](out-of-memory.md). |
| Le périphérique est perdu au démarrage | Forcez une autre API graphique avec la variable d'environnement `WGPU_BACKEND` (`dx12` ou `vulkan`), voir [Plantages et journaux](crashes-and-logs.md#choisir-lapi-graphique). |
| Un portable avec deux cartes graphiques | Faites utiliser la carte dédiée à Hammer Pro dans les paramètres graphiques de Windows ou le panneau du pilote. |

Envoyez le rapport de plantage avec le fichier journal si cela continue, voir [Signaler un bug](reporting-bugs.md).

## Pages liées

- [Mémoire insuffisante](out-of-memory.md)
- [Plantages et journaux](crashes-and-logs.md)
- [Sauvegarde automatique et plantages](../features/autosave-crash.md)

---
[Retour à l'index de la documentation](../README.md)
