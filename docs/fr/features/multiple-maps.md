[Documentation](../README.md) > Fonctionnalités > Plusieurs maps | [English](../../en/features/multiple-maps.md) | [Русский](../../ru/features/multiple-maps.md)

# Plusieurs maps

Hammer Pro garde plusieurs maps ouvertes en même temps, chacune dans son onglet au-dessus des vues, avec ses propres caméras, sa sélection, son historique d'annulation et son pointfile chargé. Vous pouvez aussi enregistrer la disposition des vues et des panneaux sous forme de dispositions nommées.

## Onglets des maps ouvertes

La barre d'onglets se trouve au-dessus des vues : un onglet par map ouverte, un `*` après le nom quand la map a des modifications non enregistrées, et le chemin complet dans l'infobulle (*Pas encore enregistrée* pour une nouvelle map).

| Geste | Effet |
| --- | --- |
| Cliquer un onglet | Affiche cette map. |
| Clic molette sur un onglet | Le ferme. |
| Bouton de fermeture (croix) d'un onglet | Le ferme. |
| **+** au bout de la barre | **Nouvelle map** dans un nouvel onglet. |
| Clic droit sur un onglet | **Fermer**, **Fermer les autres maps** (quand plusieurs maps sont ouvertes), **Copier le chemin** et **Afficher dans le dossier** (pour une map enregistrée). |

Les noms longs sont raccourcis ; la barre défile sur le côté quand les onglets ne tiennent pas.

## Commandes

| Commande | Raccourci | Effet |
| --- | --- | --- |
| **Fichier > Nouvelle map** | `Ctrl+N` | Ouvre une nouvelle map vide dans un nouvel onglet. |
| **Fichier > Ouvrir une map...** | `Ctrl+O` | Ouvre une map dans un nouvel onglet (voir plus bas). |
| **Fichier > Fermer** | `Ctrl+F4` | Ferme la map affichée, après avoir proposé d'enregistrer ses modifications (*Enregistrer les modifications de map avant de la fermer ?*). Fermer la dernière map en laisse une nouvelle vide. |
| **Fichier > Fermer les autres maps** | | Ferme toutes les autres maps, en demandant pour chaque map modifiée à tour de rôle. |
| **Fenêtre > Map suivante** | `Ctrl+Tab` | Affiche l'onglet suivant. |
| **Fenêtre > Map précédente** | `Ctrl+Maj+Tab` | Affiche l'onglet précédent. |
| **Fenêtre > Maps ouvertes** | | Liste les maps ouvertes (celle affichée est cochée) ; cliquez-en une pour l'afficher. |

Quitter Hammer Pro pose la question pour chaque map modifiée à tour de rôle.

## Ouvrir des maps

Avec **Ouvrir les maps dans de nouveaux onglets** (**Paramètres > Général**, section **Maps**, clé `workflow.open_in_new_tab`, activé par défaut) :

- une map ouverte a son propre onglet ;
- une nouvelle map « sans titre » encore intacte est remplacée au lieu de garder un onglet vide ;
- une map déjà ouverte est affichée au lieu d'être chargée deux fois.

Désactivé, une map ouverte remplace la map actuelle, après avoir proposé d'enregistrer ses modifications, comme dans Hammer.

Tout ce qui appartient à une map suit son onglet : caméras, sélection, historique d'annulation, pointfile, pivot de rotation et les données propres à la map de son fichier annexe (options de publication, problèmes ignorés...). La sauvegarde automatique enregistre toutes les maps ouvertes modifiées, pas seulement celle affichée (voir [Sauvegarde automatique et plantages](autosave-crash.md)).

## Dispositions

Une disposition est l'arrangement des vues et des panneaux : la vue de chaque quart d'écran, la position des séparations, la vue agrandie, et les panneaux du dock affichés (textures, propriétés de l'entité, visgroups, arborescence, modèles, console).

| Commande | Effet |
| --- | --- |
| **Fenêtre > Disposition > Enregistrer la disposition...** | Demande un nom (*Nom de la disposition :*, `Disposition N` proposé) et enregistre l'arrangement actuel. Une disposition du même nom est remplacée (vous êtes prévenu). 32 dispositions au plus. |
| **Fenêtre > Disposition > Réinitialiser la disposition** | Revient à l'arrangement par défaut : vues 3D, de dessus, de face et de côté, séparations centrées, aucune vue agrandie, tous les panneaux du dock affichés. |
| **Fenêtre > Disposition > Dispositions enregistrées** | Cliquez une disposition pour l'appliquer. **Supprimer** liste les dispositions pour en supprimer une. *Aucune disposition enregistrée* quand la liste est vide. |

Les dispositions sont enregistrées dans settings.toml (`[workflow.layouts]`) : elles sont les mêmes pour toutes les maps.

## MCP

`list_tabs`, `switch_tab`, `close_tab`, `new_map`, `open_map` et `list_recent_maps` agissent sur les maps ouvertes depuis les applications d'IA ; `set_view_layout` change les vues des quarts d'écran. Voir la [référence des outils MCP](../../MCP.md) (en anglais).

## Astuces

- Gardez une map de prefabs ou d'instances ouverte dans un second onglet et copiez des objets d'un onglet à l'autre avec `Ctrl+C` et `Ctrl+V`.
- Enregistrez une disposition « éclairage » avec une grande vue 3D et une disposition « brushwork » avec les quatre vues, et passez de l'une à l'autre depuis le menu **Fenêtre**.
- **Copier le chemin** dans le menu de l'onglet est le moyen le plus rapide de coller le chemin de la map dans un message ou un script.

## FAQ

**Puis-je ouvrir deux fois la même map ?** Non. Ouvrir une map déjà ouverte affiche simplement son onglet.

**Chaque map garde-t-elle son propre historique d'annulation ?** Oui. `Ctrl+Z` n'annule que les modifications de la map affichée.

**Comment revenir à une seule map à la fois, comme dans Hammer ?** Décochez **Ouvrir les maps dans de nouveaux onglets** dans **Paramètres > Général**.

## Pages liées

- [Compatibilité VMF](../getting-started/vmf-compatibility.md)
- [Visite de l'interface](../getting-started/interface-tour.md)
- [Presse-papiers](clipboard.md)
- [Fenêtre des paramètres](settings-dialog.md)

---
[Retour à l'index de la documentation](../README.md)
