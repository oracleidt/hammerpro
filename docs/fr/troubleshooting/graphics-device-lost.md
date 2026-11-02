[Documentation](../README.md) > Dépannage > Périphérique graphique perdu | [English](../../en/troubleshooting/graphics-device-lost.md)

# Périphérique graphique perdu

Le périphérique graphique est perdu quand le pilote graphique réinitialise la carte (Windows le fait quand la carte ne répond plus pendant environ 2 secondes), quand la carte n'a plus de mémoire au milieu d'une image, quand le pilote plante ou quand la carte est retirée. Plus rien ne peut être dessiné avec ce périphérique.

## Ce que fait Hammer Pro

1. Il écrit un rapport de plantage avec la raison et les dernières lignes du journal dans `%APPDATA%\HammerPro\crash\`.
2. Il sauvegarde chaque map modifiée (l'onglet affiché et les autres) à côté des sauvegardes automatiques, dans `%LOCALAPPDATA%\HammerPro\autosave\`.
3. Il affiche **Périphérique graphique perdu** avec la raison et demande **Relancer Hammer Pro maintenant ?**
   - **Oui** : Hammer Pro redémarre et rouvre tous vos onglets, les maps modifiées depuis leurs modifications sauvegardées. Un message indique que les maps ont été rouvertes.
   - **Non** : Hammer Pro se ferme. Au démarrage suivant, la fenêtre de plantage montre le rapport et propose **Ouvrir les modifications sauvegardées**.

Une perte du périphérique ne fait jamais perdre votre travail : au pire, vous retrouvez les modifications sauvegardées dans un nouvel onglet marqué comme modifié, à enregistrer par-dessus la map d'origine.

## Ce que Hammer Pro évite

- **Les images lentes des autres programmes.** Avec Vulkan sous Windows, une image que l'écran n'avait pas affichée en une seconde comptait comme un périphérique perdu. Hammer Pro attend maintenant plus longtemps (jusqu'à 30 secondes), donc un jeu, un calcul sur la carte graphique ou une image longue de l'éditeur ne l'arrêtent plus.
- **La mémoire insuffisante.** Hammer Pro garde son utilisation de la mémoire graphique sous un budget et libère de la mémoire avant que la carte soit pleine. Voir [Mémoire insuffisante](out-of-memory.md).
- **Les énormes envois d'un seul bloc.** Les lightmaps calculées et compilées sont envoyées à la carte par morceaux de 32 Mo, aucun transfert n'occupe donc la carte longtemps.

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
