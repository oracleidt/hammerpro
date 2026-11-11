[Documentation](../README.md) > Référence > Serveur MCP | [English](../../en/reference/mcp-server.md) | [Русский](../../ru/reference/mcp-server.md)

# Serveur MCP

Hammer Pro intègre un serveur MCP. MCP (Model Context Protocol) est la façon dont les applications d'IA comme Claude Code, Claude Desktop, Codex et Gemini CLI utilisent des outils : grâce à lui, un assistant IA peut lire la map ouverte dans Hammer Pro, la modifier, la vérifier, la compiler et faire des captures des vues pour contrôler son propre travail. Cette page couvre l'installation et la sécurité. La liste complète des outils avec leurs arguments est la [référence des outils MCP](../../MCP.md), disponible en anglais uniquement.

## Fonctionnement

```text
Application d'IA (Claude, Codex, Gemini)
    | lance "hammerpro.exe --mcp" et parle MCP sur son stdin et son stdout
hammerpro --mcp                     le pont : aucune fenêtre
    | 127.0.0.1, port aléatoire, jeton secret
Fenêtre Hammer Pro avec MCP actif   exécute les outils sur la map qu'elle affiche
```

- Chaque fenêtre Hammer Pro a **son propre serveur, inactif par défaut**.
- Une fois installée, l'application d'IA lance elle-même `hammerpro.exe --mcp`. Ce processus n'ouvre aucune fenêtre : il transmet les appels d'outils à une fenêtre Hammer Pro dont le serveur est actif.
- Quand aucune fenêtre n'a MCP actif, l'IA voit quand même les outils, et chaque outil répond qu'il faut ouvrir Hammer Pro et activer MCP.
- Les appels d'outils s'exécutent sur le fil de l'éditeur, entre deux images, comme un de vos clics. Le pont attend une réponse jusqu'à 150 secondes.
- Le pont parle les versions 2025-11-25, 2025-06-18, 2025-03-26 et 2024-11-05 du protocole MCP (JSON-RPC 2.0 sur stdio).

## La pastille MCP

À l'extrémité droite de la barre du haut, la pastille indique l'état du serveur de cette fenêtre :

| Pastille | Signification |
|---|---|
| **MCP inactif** | Le serveur de cette fenêtre est coupé. Rien n'écoute. |
| **MCP actif · port · clients** | Le serveur tourne, avec son port local et le nombre d'applications d'IA connectées (**1 client**, **2 clients**...). |

- **Cliquez sur la pastille** pour activer ou désactiver le serveur. Son info-bulle indique aussi le dernier outil appelé et le nombre d'appels.
- **La petite flèche** à côté de la pastille (ou un clic droit sur la pastille, **Installation dans les applications d'IA, configuration et outils**) ouvre la fenêtre MCP.
- Les mêmes commandes sont dans le menu **Outils** : **Serveur MCP** (actif/inactif), **Installer MCP dans Claude, Codex et Gemini** et **MCP...** (la fenêtre MCP).

Pour que le serveur démarre avec Hammer Pro, cochez **Démarrer le serveur MCP au lancement de Hammer Pro** dans la page **MCP** de la fenêtre Paramètres (**Outils > Options...**, `Ctrl+,`). Désactivé par défaut (paramètre `mcp.start_with_app`). La même page propose un bouton **Activer**/**Désactiver**, **Installation et configuration...** (ouvre la fenêtre MCP) et la liste **Outils proposés aux applications d'IA**.

## La fenêtre MCP (Hammer Pro MCP)

| Section | Contenu |
|---|---|
| **Serveur de cette fenêtre** | L'état (**Inactif**, ou **Actif, port local N, clients**) et un bouton **Activer**/**Désactiver**. |
| **Applications d'IA** | Le bouton **Installer dans Claude, Codex, Gemini**, puis une ligne par application avec son état et les boutons **Installer** (ou **Mettre à jour**), **Vérifier** et **Retirer**. |
| **Configuration manuelle** | Une section repliable avec des extraits prêts à coller qui utilisent le vrai chemin de votre `hammerpro.exe`, chacun avec un bouton **Copier**. |
| **Documentation MCP** | Lien vers la référence complète des outils. |

États d'une ligne d'application : **Pas encore vérifié : cliquez sur Vérifier**, **Pas installé sur cet ordinateur**, **Installé, non configuré**, **Installé et configuré**, **Configuré pour une autre copie de Hammer Pro (...) : cliquez sur Mettre à jour**, ou **Erreur : ...**.

Les programmes des applications (la commande `claude`) ne s'exécutent que lorsque vous cliquez sur **Installer**, **Vérifier** ou **Retirer**. Ouvrir la fenêtre se contente de lire leurs fichiers de configuration.

## Installer dans une application d'IA

1. Activez la pastille (**MCP inactif** devient **MCP actif**).
2. Ouvrez la fenêtre MCP (flèche à côté de la pastille, ou **Outils > Blender et MCP > MCP...**).
3. Cliquez sur **Installer dans Claude, Codex, Gemini**. Hammer Pro s'ajoute à chaque application trouvée sur cet ordinateur, dans cet ordre : Claude Code, Claude Desktop, Codex, Gemini CLI, puis vérifie chacune. Vous pouvez aussi installer une seule application avec son propre bouton **Installer**.
4. Redémarrez l'application d'IA (ou ouvrez une nouvelle session) pour qu'elle voie le nouveau serveur.

Ce que modifie chaque installation :

| Application | Ce qui est modifié |
|---|---|
| Claude Code | Lance `claude mcp add --scope user hammer-pro -- "<hammerpro.exe>" --mcp`, vérifié avec `claude mcp get hammer-pro`. `~/.claude.json` n'est jamais modifié directement. |
| Claude Desktop | `mcpServers` de `%APPDATA%\Claude\claude_desktop_config.json` (ou de la copie Microsoft Store sous `%LOCALAPPDATA%\Packages\Claude_*`). |
| Codex | `[mcp_servers.hammer-pro]` de `~/.codex/config.toml` (ou de `%CODEX_HOME%`). |
| Gemini CLI | `mcpServers` de `~/.gemini/settings.json`. |

Avant la première modification, chaque fichier est copié en `<fichier>.hammerpro.bak`. Les fichiers sont écrits via un fichier temporaire, tout le reste de leur contenu est conservé (autres serveurs, commentaires du fichier TOML) et le résultat est relu pour vérification. Un fichier JSON contenant des commentaires n'est pas touché : ajoutez l'entrée à la main.

Vous avez déplacé Hammer Pro dans un autre dossier ? Ouvrez la fenêtre MCP : les applications affichent **Configuré pour une autre copie de Hammer Pro**. Cliquez sur **Mettre à jour** (ou sur **Installer dans Claude, Codex, Gemini**).

## Configuration manuelle

Si vous préférez, ou si l'installation automatique ne peut pas modifier un fichier, ajoutez le serveur vous-même (la fenêtre MCP affiche ces extraits avec votre vrai chemin) :

Claude Code :

```bash
claude mcp add --scope user hammer-pro -- "C:\Program Files\Hammer Pro\hammerpro.exe" --mcp
```

Claude Desktop (`claude_desktop_config.json`) et Gemini CLI (`~/.gemini/settings.json`) :

```json
{
  "mcpServers": {
    "hammer-pro": {
      "command": "C:\\Program Files\\Hammer Pro\\hammerpro.exe",
      "args": ["--mcp"]
    }
  }
}
```

Codex (`~/.codex/config.toml`) :

```toml
[mcp_servers.hammer-pro]
command = 'C:\Program Files\Hammer Pro\hammerpro.exe'
args = ["--mcp"]
```

Toute autre application MCP : un serveur stdio avec la commande `hammerpro.exe` et l'argument `--mcp`.

## Sécurité

- **Inactif par défaut**, fenêtre par fenêtre. Rien n'écoute tant que vous n'activez pas la pastille.
- **Local uniquement** : le serveur écoute sur `127.0.0.1`, jamais sur le réseau, et refuse toute connexion qui ne vient pas de cet ordinateur.
- **Jeton secret** : chaque fois qu'un serveur démarre, il génère un jeton aléatoire de 32 octets. Chaque connexion doit l'envoyer en premier, sinon elle est fermée. Le jeton n'est écrit que dans la fiche d'instance de la fenêtre, `%LOCALAPPDATA%\HammerPro\mcp\instances\<pid>.json`, dans votre propre profil. La fiche est supprimée quand le serveur s'arrête ou que Hammer Pro se ferme ; les fiches des fenêtres plantées sont ignorées puis nettoyées.
- **Limites** : messages de 8 Mo au maximum, 8 connexions simultanées par fenêtre, un délai maximal sur chaque appel d'outil.
- **Aucun coût au repos** : le fil du serveur dort jusqu'à l'arrivée d'une connexion (sans interrogation périodique) ; une fenêtre avec MCP actif et aucune IA connectée ne consomme pas de processeur. Un long appel d'outil qui tourne en arrière-plan (import par lot, conversion de texture) réveille la fenêtre quand il se termine au lieu de la faire redessiner pendant ce temps.
- **Chemins de fichiers** : un outil qui prend un fichier ou un dossier refuse les chemins réseau (UNC, `\\serveur\partage`) et les chemins de périphérique (`\\?\`, `\\.\`, `CON`, `NUL`, `COM1`...), pour qu'un chemin glissé dans une consigne ne puisse pas faire envoyer vos identifiants Windows à un autre ordinateur. Les chemins relatifs restent dans leur dossier (pas de `..`), et les outils qui écrivent à côté de la map (graphes de shader, textures, images, sons, tables de couleurs) n'acceptent que le dossier de la map et les dossiers de textures personnalisés.
- **Aucun programme sans votre demande, aucun fichier arbitraire** : les outils ne lancent aucune commande d'eux-mêmes. Les maps ne sont ouvertes et enregistrées qu'en `.vmf` ; écraser un autre fichier existant exige un `overwrite` explicite. Le jeu n'est lancé que si l'IA le demande explicitement. Les compilations utilisent le compilateur choisi pour le jeu (celui de Hammer Pro par défaut) ; créer ou changer un compilateur qui lance d'autres programmes (`create_compiler`, `set_compiler` avec des étapes) exige `confirm: true`. Voir [Choisir un compilateur](../compiler/compilers.md).
- **Confirmation** : `run_action` lance n'importe quelle commande des menus, sauf quelques-unes qui exigent `confirm: true`, que l'IA ne transmet que si vous avez demandé exactement cela : Quitter, désactiver MCP, installer MCP dans d'autres applications, publier, lancer le jeu ou lui parler, compiler avec le jeu, éditer une instance dans une autre fenêtre, ouvrir le navigateur web et les commandes qui affichent une boîte de dialogue de fichier.
- **Paramètres** : `set_setting` ne change jamais les chemins de fichiers, dossiers, programmes, lignes de commande, dossiers de contenu du jeu ni les paramètres MCP : vous les changez dans la fenêtre Paramètres. Ajouter ou retirer un FGD supplémentaire (`.fgd` uniquement) exige `confirm: true`.
- Les outils d'automatisation de l'interface (`ui_click`, `ui_key`, `ui_type`...) n'agissent que dans la fenêtre Hammer Pro.
- Les exécutions automatisées (`--script`) ne lancent jamais le jeu, confirmation ou non.

## Plusieurs fenêtres

Avec plusieurs fenêtres Hammer Pro, le pont utilise celle avec MCP actif qui a eu le focus en dernier, et la garde tant qu'il tourne. L'IA peut lister les fenêtres avec `list_instances` et en changer avec `use_instance`. Pour fixer une fenêtre dans la configuration d'une application, donnez à la variable d'environnement `HAMMERPRO_INSTANCE` son identifiant de processus.

## Annulation

Chaque modification faite par l'IA est **une étape d'annulation** nommée "MCP :" suivi de ce qui a été fait (par exemple **MCP : application de matériau**), visible dans le menu **Édition** et l'historique d'annulation. `Ctrl+Z` l'annule comme n'importe laquelle de vos modifications. L'IA peut aussi annuler et rétablir (`undo`, `redo`, `undo_steps`, `redo_steps`, ou `run_action` avec `undo` et `redo`) et lire l'historique (`get_history`). Dans une session de Team Mapping, ils passent par l'historique partagé de la session exactement comme `Ctrl+Z` et `Ctrl+Y`, une étape par appel, pour que les autres participants voient le changement.

## Catégories d'outils

| Catégorie | Ce que couvrent les outils |
|---|---|
| Help | `get_capabilities` : tous les outils par catégorie avec un exemple d'appel. L'IA commence par là. |
| Map | `get_map_info` : fichier, modifications non enregistrées, compteurs, limites, sélection, étapes d'annulation, état du contenu et de la compilation. |
| Files | Ouvrir un `.vmf`, enregistrer la map. |
| Brushes and objects | Lister les solides, créer des brushes (bloc, coin, cylindre, pointe, sphère, arche, tore, escalier, colimaçon, rampe, pilier, encadrement de porte), appliquer des matériaux, déplacer, tourner, supprimer. |
| Entities | Lister, lire, créer des entités et régler leurs keyvalues. |
| Selection | Lire, changer et vider la sélection. |
| History | Annuler et rétablir. |
| Clipboard | Lire le texte VMF d'objets et coller du texte VMF. |
| Assets | Chercher matériaux, modèles, sons et particules, infos et aperçus de matériaux et de modèles, édition de VMT, import d'images, calques UV et alignement de textures. |
| Éditeur de shaders | L'éditeur d'images à nœuds : listes de nœuds et de préréglages, création, lecture, modification, vérification, rendu et écriture de graphes `.hpshader` en fichiers VTF et VMT, et ouverture de la fenêtre de l'éditeur. Voir [Éditeur de shaders](../features/shader-editor.md). |
| Map health | Vérifier les problèmes, les corriger, limites du moteur, statistiques de la map, faces cachées. |
| Compile | Compiler avec un préréglage et suivre la progression. |
| Views | Caméra, captures d'une vue, disposition des quadrants, modes de rendu 3D, vue joueur, zoom et déplacement 2D. |
| Actions | Lister et lancer toutes les commandes des menus et raccourcis. |
| Settings | Lire et changer les paramètres et les raccourcis clavier. |
| Ui automation | Capture d'une fenêtre, description de l'interface, souris et clavier dans la fenêtre. |
| Windows | Lister les fenêtres Hammer Pro et en choisir une. |
| Editing | Dupliquer, copies en série, transformer, redimensionner, retourner, aligner sur la grille, aligner, découper, carve, contours de face creusés ou extrudés, biseaux d'arêtes (arrondis, chanfreins, gorges, remplissages de coins intérieurs), creuser, fusionner, édition de sommets, primitives, pièces, escaliers, hint et nodraw, lier à une entité, textures des faces (justifier, ajuster, envelopper), remplacement de matériaux. |
| Groups and visibility | Sélection par classe, nom, matériau ou boîte, groupes, visgroups, masquer et isoler, cordons, couleurs d'affichage. |
| Grids | Modes de grille, grilles personnalisées (créer, modifier, supprimer, attacher). |
| Displacements | Créer, détruire, peindre, traits de sculpture, bruit, échelle, alpha, drapeaux, tags, subdiviser, coudre. |
| Instances and prefabs | func_instance, paramètres d'instance, fusion, prefabs, sprinkle, lancers de rayons. |
| Entities and io | Classes d'entités, keyvalues, spawnflags, sorties, graphe d'I/O, rechercher et remplacer du texte, rapport d'entités, targetnames. |
| Compile and play | Toutes les options de compilation, log et statistiques de compilation, historique, fuites, pointfiles, portails, aperçu de l'éclairage précalculé, lancement du jeu. |
| Content and release | Rapport de dépendances, Construire et enregistrer en contenu, checklist, publication sur le Workshop, collecte des ressources de la map, fichiers inutilisés. |
| Documents | Onglets des maps ouvertes, nouvelle map, maps récentes, import et export, Décompilateur de BSP, nav mesh, propriétés de la map. |
| BSP Sniffer | Lancer, annuler et suivre une opération du [BSP Sniffer](../features/bsp-sniffer.md) (statut, journal, rapport) et lire ou changer chaque réglage. Demande le réglage `authorized`, que seul l'utilisateur doit activer. |
| History and clipboard | Historique d'annulation, plusieurs étapes d'annulation ou de rétablissement, copier, couper, coller, collage spécial. |
| Overlays and decals | Lister et créer des overlays et des décals. |
| Paths | Chaînes d'entités de chemin. |
| Routes | Créer, lire, lister, modifier, supprimer et couper des [routes](../features/road-tool.md), et relier l'extrémité d'une route au côté d'une autre avec des évasements. |
| Skybox | Sky cameras, aperçu, interrupteur de skybox. |
| Entity definitions | Infos FGD, ajouter, retirer, recharger, valider des fichiers FGD. |
| Games | Lister les jeux pris en charge, lire et changer le jeu de la map. |
| Diagnostics | Les rapports de plantage du dossier crash (liste et lecture de l'un d'eux) et les dernières lignes du journal de Hammer Pro. Voir [Plantages et journaux](../troubleshooting/crashes-and-logs.md). |
| Tool options | Lire et changer les options des outils d'édition. |
| Placement | Forme exacte de tout objet ou modèle mesurée sur le vrai maillage (`get_object_shape`), dessins cotés (`render_shape`), pose sur, contre ou à côté de n'importe quoi avec un vrai contact (`place_object`, `drop_to_surface`), vérification de la pose (`check_placement`) et recherche des objets flottants ou enfoncés (`find_floating_objects`). |

Les noms de catégories sont ceux de la référence, en anglais. La liste complète de chaque outil avec sa description et ses arguments se trouve dans [docs/MCP.md](../../MCP.md) (en anglais uniquement).

## Pages liées

- [Utiliser Hammer Pro avec un assistant IA](../guides/mcp-with-ai-assistant.md)
- [Référence des outils MCP (anglais)](../../MCP.md)
- [Ligne de commande](command-line.md)
- [Commandes de script](script-commands.md)
- [Référence des paramètres](settings.md)
- [Annulation et historique](../features/undo-history.md)

---
[Retour à l'index de la documentation](../README.md)
