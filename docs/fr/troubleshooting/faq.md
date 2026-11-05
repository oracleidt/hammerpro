[Documentation](../README.md) > Dépannage > FAQ | [English](../../en/troubleshooting/faq.md) | [Русский](../../ru/troubleshooting/faq.md)

# FAQ

Des réponses courtes aux questions les plus fréquentes, avec des liens vers les pages qui détaillent.

## Généralités

**Qu'est-ce que Hammer Pro ?** Un éditeur de maps pour le moteur Source, pensé d'abord pour Garry's Mod. Il ouvre et enregistre des fichiers `.vmf` standards, dessine avec le GPU et compile les maps avec son propre compilateur intégré. Voir [Installation](../getting-started/installation.md).

**Est-il gratuit ?** Oui. Hammer Pro est un logiciel libre et open source sous licence MIT (voir [LICENSE](../../../LICENSE)).

**Hammer Pro est-il fait par Valve ou Facepunch ?** Non. Il n'est affilié ni à Valve ni à Facepunch Studios, et n'inclut ni ne redistribue aucun code ou contenu de jeu de Valve.

**Remplace-t-il Hammer et Hammer++ ?** Il fait ce qu'ils font et davantage, mais vous n'avez pas à choisir : le même `.vmf` s'ouvre dans les trois, et les données de Hammer++ sont conservées. Voir [Compatibilité VMF](../getting-started/vmf-compatibility.md).

**Pour quels jeux puis-je mapper ?** Garry's Mod en priorité. De nombreux autres jeux Source sont pris en charge comme cibles de compilation (Half-Life 2 et ses épisodes, Counter-Strike: Source, Team Fortress 2, Portal, Left 4 Dead 2, Black Mesa et d'autres). Voir [Jeux cibles](../compiler/target-games.md).

**Quels systèmes sont pris en charge ?** Windows 10 et 11 (64 bits) avec un GPU compatible Vulkan, DirectX 12 ou Metal. Le code se compile aussi sous Linux.

## Configuration

**Ai-je besoin de VBSP, VVIS et VRAD ?** Non. Hammer Pro compile les maps lui-même. Voir [Vue d'ensemble du compilateur](../compiler/overview.md).

**Hammer Pro ne trouve pas Garry's Mod.** **Outils > Options... > Jeu**, puis **Détecter** ou **Parcourir...** jusqu'au dossier qui contient `garrysmod` et `bin`. Voir [Configuration du jeu](../getting-started/game-setup.md).

**Je viens de Hammer : puis-je garder mes raccourcis ?** Oui. Choisissez **Hammer** ou **Hammer++** dans l'assistant de configuration (**Aide > Assistant de configuration...**) ou dans **Paramètres > Clavier**. Voir [Raccourcis clavier](../features/keyboard-shortcuts.md).

**Comment passer de l'anglais au français ?** **Affichage > Langue**, à tout moment ; toute l'interface change aussitôt. `--lang en` ou `--lang fr` en ligne de commande la force pour un lancement. Voir [Thèmes et langues](../features/themes-languages.md).

**Où sont mes paramètres ?** Dans `%APPDATA%\HammerPro\settings.toml`. Voir [Fichiers et dossiers](../reference/files-and-folders.md).

## Mapping

**Mes textures du Workshop ou d'un addon ne s'affichent pas.** Vérifiez **Monter les addons** et **Monter les téléchargements du Workshop** dans **Paramètres > Jeu**, puis **Appliquer et recharger le contenu**.

**Puis-je utiliser mes propres textures PNG ou JPG ?** Oui : ajoutez leur dossier dans **Paramètres > Jeu > Dossiers de textures personnalisés**. Les images servent de matériaux tout de suite et sont converties en VTF + VMT à la compilation. Voir [Dossiers de textures personnalisés](../features/custom-texture-folders.md).

**Puis-je ouvrir plusieurs maps ?** Oui, chacune dans son onglet ; le copier-coller fonctionne entre elles et même entre deux fenêtres de Hammer Pro. Voir [Plusieurs maps ouvertes](../features/multiple-maps.md).

**J'ai supprimé quelque chose par erreur.** `Ctrl+Z`. Chaque modification s'annule, jusqu'à 500 étapes par défaut. Voir [Annulation et historique](../features/undo-history.md).

## Compiler et jouer

**Comment tester ma map ?** Enregistrez-la, puis appuyez sur `F9` : elle se compile et Garry's Mod démarre dessus. Voir [Votre première map](../getting-started/first-map.md) et [Compiler et tester dans Garry's Mod](../guides/compile-and-test-gmod.md).

**Ma map fuit.** Suivez le chemin de fuite tracé dans les vues et bouchez le trou avec un brush du monde. Voir [Erreurs courantes](common-errors.md).

**Les joueurs voient des damiers violets et noirs.** Le contenu personnalisé n'a pas été livré avec la map. Voir [Erreurs courantes](common-errors.md) et [Construire et enregistrer en contenu](../features/build-content-release.md).

## Applications d'IA

**Un assistant IA peut-il modifier ma map ?** Oui, grâce au serveur MCP intégré (désactivé par défaut, activé fenêtre par fenêtre avec la pastille **MCP**). Chaque modification faite par l'IA est une étape d'annulation. Voir [Serveur MCP](../reference/mcp-server.md) et [Utiliser Hammer Pro avec un assistant IA](../guides/mcp-with-ai-assistant.md).

## Problèmes

**Hammer Pro a planté.** Vos modifications sont en général sauvegardées : voir [Plantages et journaux](crashes-and-logs.md).

**L'éditeur est lent.** Voir [Performances de l'éditeur](performance.md).

**Mon statut Discord n'affiche pas ce que je mappe.** Voir [Le statut Discord ne s'affiche pas](discord-presence.md).

**Comment signaler un bug ?** Voir [Signaler un bug](reporting-bugs.md).

## Pages liées

- [Erreurs courantes](common-errors.md)
- [Plantages et journaux](crashes-and-logs.md)
- [Performances de l'éditeur](performance.md)
- [Signaler un bug](reporting-bugs.md)

---
[Retour à l'index de la documentation](../README.md)
