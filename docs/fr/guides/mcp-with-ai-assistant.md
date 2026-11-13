[Documentation](../README.md) > Guides > Utiliser Hammer Pro avec un assistant IA | [English](../../en/guides/mcp-with-ai-assistant.md) | [Русский](../../ru/guides/mcp-with-ai-assistant.md)

# Utiliser Hammer Pro avec un assistant IA

Le serveur MCP intégré de Hammer Pro permet à un assistant IA (Claude Code, Claude Desktop, Codex, Gemini CLI ou toute application MCP) de lire et modifier la map ouverte dans une fenêtre Hammer Pro : construire des pièces, placer des entités, corriger des problèmes, compiler et regarder des captures des vues pour vérifier son travail. Ce guide l'installe et montre comment travailler avec. Les détails d'installation et la sécurité sont dans [Serveur MCP](../reference/mcp-server.md).

## Étape 1 : activer le serveur

À l'extrémité droite de la barre du haut, cliquez sur la pastille **MCP inactif**. Elle devient **MCP actif · port · clients** : le serveur de cette fenêtre tourne. Il est inactif par défaut dans chaque fenêtre ; pour l'activer à chaque lancement, cochez **Démarrer le serveur MCP au lancement de Hammer Pro** dans la page **MCP** des paramètres (**Outils > Options...**).

## Étape 2 : installer Hammer Pro dans votre application d'IA

1. Cliquez sur la petite flèche à côté de la pastille (ou **Outils > Intégrations > MCP...**) pour ouvrir la fenêtre MCP.
2. Cliquez sur **Installer dans Claude, Codex, Gemini**. Hammer Pro s'ajoute à chaque application prise en charge qu'il trouve, puis vérifie chacune. La ligne de chaque application affiche **Installé et configuré** quand tout a marché.
3. Redémarrez l'application d'IA, ou ouvrez une nouvelle session, pour qu'elle voie le serveur `hammer-pro`.

Cette étape ne se fait qu'une fois par ordinateur (à refaire seulement si vous déplacez Hammer Pro dans un autre dossier : cliquez sur **Mettre à jour**). Pour une application absente de la liste, utilisez la section **Configuration manuelle** : un serveur stdio avec la commande `hammerpro.exe` et l'argument `--mcp`.

## Étape 3 : demander quelque chose

Ouvrez votre map dans Hammer Pro, vérifiez que la pastille affiche **MCP actif**, puis parlez à l'assistant normalement. Par exemple :

- "Regarde ma map Hammer Pro et décris-la."
- "Ajoute un sol de 512 x 512 en `dev/dev_measuregeneric01b`, quatre murs de 128 de haut et une lumière au milieu."
- "Place un `info_player_start` dans la pièce et vérifie les problèmes de la map."
- "Fais l'escalier du bâtiment nord avec des marches de 8 unités."
- "Trouve toutes les faces qui utilisent un matériau manquant et remplace-le par `concrete/concretefloor001a`."
- "Compile avec le préréglage Brouillon et dis-moi si la map fuit."

L'assistant commence par `get_capabilities` et `get_map_info`, utilise les outils dont il a besoin, et peut faire des captures des vues (`screenshot_view`) ou de toute la fenêtre (`screenshot_window`) pour vérifier le résultat.

## Étape 4 : relire et annuler

- Chaque modification de l'assistant est **une étape d'annulation** nommée "MCP :" suivi de ce qui a été fait (par exemple **MCP : application de matériau**). `Ctrl+Z` l'annule comme vos propres modifications ; l'historique d'annulation les montre toutes.
- La map n'est pas enregistrée tant que vous, ou l'assistant à votre demande, ne l'enregistrez pas. La sauvegarde automatique continue de fonctionner normalement.
- Regardez les vues pendant qu'il travaille : les appels d'outils s'exécutent entre deux images de l'éditeur, comme vos clics.

## Ce que l'assistant peut faire ou non

- Il n'atteint que les fenêtres dont la pastille est sur **MCP actif**, et seulement depuis cet ordinateur (adresse locale, jeton secret).
- Il ne lance aucun programme de lui-même et n'ouvre ou n'enregistre que des maps `.vmf`. Écraser un autre fichier existant exige un écrasement explicite. Il ne peut créer ou changer un compilateur qui lance d'autres programmes qu'avec une confirmation, quand vous l'avez demandé.
- Les commandes sensibles (quitter, désactiver MCP, installer MCP dans d'autres applications, publier, lancer le jeu, boîtes de dialogue de fichiers...) exigent une confirmation que l'assistant ne doit donner que si vous avez demandé exactement cela.
- Il ne peut pas changer les chemins de fichiers, programmes, lignes de commande, dossiers de contenu du jeu ni les paramètres MCP : vous le faites dans la fenêtre Paramètres.

## Plusieurs fenêtres Hammer Pro

Avec plusieurs fenêtres, l'assistant utilise celle avec MCP actif qui a eu le focus en dernier. Demandez-lui de "lister les fenêtres Hammer Pro" puis "d'utiliser celle avec gm_city" (outils `list_instances` et `use_instance`). Dans une même fenêtre, il peut aussi passer d'une map ouverte à l'autre (`list_tabs`, `switch_tab`).

## Placer des props et des objets avec précision

L'assistant n'a pas à deviner les tailles d'après des boîtes englobantes. Demandez-lui de placer des choses et il utilise les outils de placement, qui mesurent la vraie géométrie de tout : le maillage du modèle après ses angles et son échelle, les brushes, les déplacements, les overlays, les instances et les groupes.

- « Mets un canapé contre le mur nord du salon, sur le sol. » L'assistant mesure le canapé (`get_object_shape` : vraie taille, hauteur de l'origine au-dessus des pieds), puis le place collé au mur et sur le sol (`place_object` avec `against`, puis `drop`).
- « Pose un baril sur la table. » Il utilise `place_object` avec `on` : le point le plus bas du baril touche exactement le plateau, sans espace ni chevauchement.
- « Gare la voiture sur la colline. » La voiture se pose sur la vraie surface déplacée, pas sur le brush en dessous.
- « Trouve tout ce qui flotte ou est enfoncé dans cette rue et corrige-le. » `find_floating_objects` vérifie chaque prop sur son vrai maillage, garde les lampes murales et les panneaux qui touchent un mur, et descend ou remonte les autres en une seule étape d'annulation.

`render_shape` donne à l'assistant un dessin d'un modèle ou d'une zone vu de plusieurs côtés avec les cotes écrites dessus, et `check_placement` lui dit pour chaque objet s'il repose, flotte, est enfoncé ou entre dans un autre objet. Le guide complet est dans [docs/MCP.md](../../MCP.md#place-things-precisely) (en anglais).

## Optimiser ce que l'assistant a construit

Demandez à l'assistant de finir par `optimize_map` : il compile la visibilité en arrière-plan, liste ce qui n'est pas optimisé (visleaves, func_detail, hints, areaportals, lumières) et, avec `apply: "safe"`, corrige la partie sûre en une étape d'annulation et donne les visleaves et le PVS avant et après. Voir [Optimiser une map](optimizing-a-map.md).

## Astuces

- Soyez concret : tailles en unités, noms de matériaux, classes d'entités. L'assistant peut aussi les chercher (`search_materials`, `search_models`, `list_entity_classes`).
- Demandez-lui de vérifier son travail : "fais une capture de la vue 3D", "lance la vérification des problèmes".
- Pour les grosses modifications, enregistrez d'abord, ou demandez une étape à la fois pour pouvoir annuler facilement.
- L'assistant peut aussi piloter toutes les commandes des menus et tous les paramètres (`run_action`, `set_setting`), et même l'interface elle-même (`describe_ui`, `ui_click`...) pour ce qui n'a pas d'outil dédié.

## FAQ

**L'assistant dit qu'aucune fenêtre Hammer Pro n'a MCP actif.** Cliquez sur la pastille dans la fenêtre Hammer Pro que vous voulez utiliser.

**L'application ne liste pas les outils hammer-pro.** Redémarrez l'application ou ouvrez une nouvelle session après l'installation. Dans la fenêtre MCP, cliquez sur **Vérifier** sur sa ligne.

**Peut-il compiler et lancer le jeu ?** Il compile avec le compilateur choisi pour le jeu (celui de Hammer Pro par défaut) quand vous le demandez, et peut en choisir ou en régler un autre (les outils de Valve, le vôtre) quand vous le demandez. Il ne lance le jeu que si vous le demandez explicitement, et jamais lors des exécutions automatisées.

## Pages liées

- [Serveur MCP](../reference/mcp-server.md)
- [Référence des outils MCP (anglais)](../../MCP.md)
- [Annulation et historique](../features/undo-history.md)
- [Vérifier les problèmes](../features/check-problems.md)
- [Compiler et tester dans Garry's Mod](compile-and-test-gmod.md)

---
[Retour à l'index de la documentation](../README.md)
