[Documentation](../README.md) > Fonctionnalités > Visgroups | [English](../../en/features/visgroups.md) | [Русский](../../ru/features/visgroups.md)

# Visgroups

Les visgroups sont des ensembles nommés d'objets que vous affichez et masquez d'un coup : le toit, les props de détail, les triggers d'une zone. Hammer Pro a vos propres visgroups (enregistrés dans le VMF, compatibles Hammer et Hammer++) et des visgroups automatiques construits selon la nature des objets (géométrie du monde, triggers, lumières, brushes outils...).

## Accès

- L'onglet **Visgroups** du dock de droite, affiché ou masqué avec **Affichage > Panneaux > Visgroups** (affiché par défaut, `ui.show_visgroups`).
- **Affichage > Déplacer vers un visgroup...** ouvre la fenêtre « Déplacer vers un visgroup » de Hammer pour la sélection.
- **Affichage > Créer un visgroup des objets masqués** transforme les objets masqués avec **Masquer la sélection** en visgroup.
- L'onglet **Visgroups** du panneau d'entité liste les visgroups de l'entité sélectionnée (voir [Propriétés des entités](entity-properties.md)).

L'onglet a trois sections : vos visgroups, les **Cordons** (voir [Cordons](cordons.md)) et les **Visgroups automatiques**.

## Vos visgroups

### Barre de l'onglet

| Bouton | Effet |
| --- | --- |
| **+** | **Nouveau visgroup à partir de la sélection** (ou **Nouveau visgroup vide** si rien n'est sélectionné). Le nouveau visgroup reçoit un nom unique et passe en mode renommage. |
| Œil | **Afficher tous les visgroups**. |
| Flèche vers le bas | **Tout déplier**. |
| Flèche vers le haut | **Tout replier**. |
| Dossier | **Déplacer vers un visgroup...** (demande une sélection). |
| Compteur | Nombre de visgroups, imbriqués compris. |

### Une ligne

| Élément | Effet |
| --- | --- |
| Flèche | Déplie ou replie les enfants. |
| Case à cocher | Affiche ou masque le visgroup et ses enfants. Elle est mixte quand un enfant est masqué ; un clic sur un visgroup mixte ou masqué l'affiche entièrement. |
| Pastille de couleur | Cliquez pour changer la **Couleur du visgroup**. Les changements successifs du sélecteur fusionnent en une étape d'annulation. |
| Nom | Double-cliquez pour renommer (`Entrée` valide, `Échap` annule). |
| Compteur | Objets directement dans le visgroup. L'infobulle donne aussi le total avec ses enfants. |
| Clic | Met la ligne en évidence. |
| Glisser sur un autre visgroup | En fait un enfant de ce visgroup. Déposez sous les lignes (« Déposer ici pour placer au premier niveau ») pour le remettre au premier niveau. Un visgroup ne peut pas aller dans ses propres enfants. |

### Menu du clic droit d'une ligne

| Entrée | Effet |
| --- | --- |
| **Sélectionner les membres** | Sélectionne les objets visibles du visgroup et de ses enfants. Les membres masqués ne sont pas sélectionnés, comme dans Hammer. |
| **Ajouter la sélection** | Ajoute les objets sélectionnés au visgroup. |
| **Retirer la sélection** | Retire les objets sélectionnés du visgroup. |
| **Déplacer la sélection ici** | Place la sélection dans ce visgroup et la retire de ses autres visgroups. |
| **Monter** / **Descendre** | Change l'ordre du visgroup parmi ses voisins. |
| **Nouveau visgroup enfant** | Crée un visgroup vide sous celui-ci. |
| **Renommer** | Le renomme. |
| **Supprimer le visgroup** | Le supprime. Ses enfants remontent dans son parent et ses objets restent dans la map : ils quittent seulement le visgroup. |

Chaque modification (création, nom, couleur, déplacement, affichage, membres) est une étape d'annulation.

## Déplacer vers un visgroup

**Affichage > Déplacer vers un visgroup...** (ou le bouton dossier de l'onglet) ouvre une fenêtre : « Déplacer N objets sélectionnés vers : » avec l'arbre de vos visgroups et **Nouveau visgroup...** (avec un champ **Nom :**). Options :

| Option | Effet |
| --- | --- |
| **Retirer des autres visgroups** | Les objets quittent tous leurs autres visgroups. |
| **Masquer le visgroup** | Masque le visgroup ensuite (réaffichez-le avec sa case à cocher). Les objets sont alors désélectionnés. |

**Déplacer** applique le tout en une étape d'annulation. Les valeurs par défaut des deux options viennent des paramètres plus bas.

## Visgroup des objets masqués

**Affichage > Créer un visgroup des objets masqués** (le « Convert QuickHide objects to visgroup » de Hammer++) déplace tous les objets masqués avec **Masquer la sélection** (`H`) dans un nouveau visgroup nommé `_FromQuickHide(N)`, retire leur masquage et masque le visgroup à la place. Les objets restent masqués, mais vous pouvez maintenant les réafficher par leur nom. Une étape d'annulation. Le bouton **En visgroup** de la ligne **Objets masqués** (visgroups automatiques) fait la même chose.

## Visgroups automatiques

Les visgroups automatiques sont calculés à partir de la map : chaque ligne a une case à cocher et un compteur. Les groupes qui ont des membres se déplient. Les enfants sans membre ne sont pas listés.

| Groupe | Enfants |
| --- | --- |
| **Géométrie du monde** (solides qui n'appartiennent à aucune entité) | **Ciel**, **Displacements**, **Eau**, **Brushes avec faces nodraw** |
| **Détails du monde** (func_detail et props) | **func_detail**, **Props** |
| **Entités** | **Entités ponctuelles**, **Entités de brush**, **Triggers**, **Lumières**, **PNJ**, **Sprites**, **Cubemaps**, **Overlays et décals**, **Instances** |
| **Brushes outils** (toutes les faces ont une texture outil) | **Clips**, **Hint et skip**, **Areaportals**, **Occluders**, **Invisibles**, **Échelles**, **Autres textures outils** |
| **Objets masqués** | Les objets masqués avec **Masquer la sélection**, avec les boutons **Tout afficher** et **En visgroup**. |

Une deuxième liste, **Depuis les définitions d'entités (FGD)**, montre les groupes `@AutoVisGroup` des FGD chargés avec leur nombre d'entités (« Définitions d'entités non chargées : groupes FGD indisponibles » sans FGD). Le bouton œil de l'en-tête (**Afficher tous les objets masqués par un visgroup automatique**) réaffiche tout. Les compteurs se mettent à jour un instant après chaque modification de la map.

## Couleurs des visgroups

Chaque nouveau visgroup reçoit une couleur agréable. Avec **Outils > Options... > Rendu > Couleurs des groupes de visibilité** activé (`render.use_visgroup_colors`, désactivé par défaut), les brushes sont dessinés dans les vues 2D de la couleur de leur visgroup. Une couleur d'affichage choisie pour un objet passe avant (voir [Couleurs d'affichage](display-colors.md)).

## Options

**Outils > Options... > Cordons et visgroups**, section **Déplacer vers un visgroup** (`[visibility]` dans `settings.toml`) :

| Option | Effet | Défaut |
| --- | --- | --- |
| **Retirer les objets de leurs autres visgroups** (`move_remove_others`) | Valeur par défaut de **Retirer des autres visgroups**. | Activé |
| **Masquer le visgroup ensuite** (`move_hide`) | Valeur par défaut de **Masquer le visgroup**. | Désactivé |

## Astuces

- Les objets masqués par un visgroup sont exclus de la compilation, sauf si **Compiler les objets masqués** est coché, comme dans Hammer (voir [Fenêtre Compiler et lancer](../compiler/compile-window.md)).
- Les objets masqués ne peuvent pas être sélectionnés : affichez d'abord le visgroup.
- Outils MCP : `list_visgroups`, `create_visgroup`, `edit_visgroup`, `delete_visgroup`, `set_visgroup_members`, `move_to_visgroup`, `set_visgroup_visible`, `hidden_to_visgroup`, `select_group` (voir la [référence des outils MCP](../../MCP.md), en anglais).

## FAQ

**J'ai cliqué sur Sélectionner les membres et rien n'a été sélectionné.** Tous les membres sont masqués. Affichez d'abord le visgroup avec sa case.

**Supprimer un visgroup supprime-t-il ses objets ?** Non. Les objets le quittent seulement ; ses visgroups enfants remontent d'un niveau.

**Les visgroups automatiques sont-ils enregistrés dans la map ?** Les groupes sont recalculés à partir de la map à chaque fois. Ce qu'ils masquent est enregistré sur chaque objet dans la clé `visgroupautoshown` de Hammer, donc un visgroup automatique masqué le reste après l'enregistrement. Afficher ou masquer un visgroup automatique est une étape d'annulation.

**Hammer lira-t-il mes visgroups ?** Oui, ce sont les blocs `visgroups` et les clés `visgroupid` de Hammer.

## Pages liées

- [Masquer et isoler](hide-isolate.md)
- [Cordons](cordons.md)
- [Groupes](groups.md)
- [Arborescence](outliner.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
