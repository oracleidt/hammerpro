[Documentation](../README.md) > Dépannage > Mémoire insuffisante | [English](../../en/troubleshooting/out-of-memory.md)

# Mémoire insuffisante

Deux sortes de mémoire peuvent manquer : la mémoire de la carte graphique (VRAM), qui contient la géométrie, les textures, les modèles et les lightmaps des maps affichées, et la mémoire de l'ordinateur (RAM), qui contient les maps elles-mêmes, l'historique d'annulation et les listes de contenu. Hammer Pro surveille les deux et libère ce qu'il peut avant que quelque chose ne casse.

## Mémoire graphique (VRAM)

### Ce que Hammer Pro fait tout seul

Hammer Pro mesure la mémoire graphique qu'il utilise (Windows la donne pour le programme ; sur les autres systèmes, elle est estimée d'après ce que Hammer Pro a créé) et la compare à un **budget**. Toutes les demi-secondes :

| Utilisation du budget | Ce qui se passe |
|---|---|
| Moins de 85 % | Rien. Les maps des autres onglets restent sur la carte graphique, le changement d'onglet est donc instantané, tant qu'elles tiennent dans les trois quarts du budget avec la map affichée. |
| Plus de 85 % | Les maps gardées dans les autres onglets sont retirées de la carte graphique (elles se rechargent quand vous affichez leur onglet), les textures et modèles qu'aucune map ouverte n'utilise sont libérés, et l'aperçu de l'éclairage calculé est libéré quand aucune vue ne l'affiche (il est recalculé quand vous l'affichez). Un avertissement le signale une fois. |
| Au-delà du budget, ou après une erreur de mémoire insuffisante | Les textures sont rechargées à la moitié de leur taille, une étape toutes les 2 secondes, jusqu'à 256 pixels, et les miniatures des navigateurs sont libérées. L'avertissement indique que les textures sont affichées en plus basse résolution. |
| Moins de la moitié du budget pendant 20 secondes | Les textures reviennent à leur pleine taille, une étape à la fois. |

Quand la carte graphique refuse une allocation, Hammer Pro garde l'ancien tampon au lieu de dessiner avec un tampon cassé, libère de la mémoire, puis recharge la map sur la carte : la vue ne reste jamais vide ou cassée, elle montre seulement moins de détails un instant.

Quitter la vue joueur libère le monde compilé, ses lightmaps et son ciel. Relancer un calcul de l'éclairage remplace le précédent au lieu de s'y ajouter, et le résultat est envoyé à la carte graphique par morceaux de 32 Mo.

### Le budget

**Paramètres > Rendu > Mémoire graphique** :

| Paramètre | Ce qu'il fait |
|---|---|
| **Budget de mémoire graphique** | Quantité de mémoire graphique que Hammer Pro peut utiliser. **Automatique** (0, par défaut) utilise 90 % de ce que Windows donne au programme, ou 3 Go (1,5 Go sur une puce graphique intégrée) quand le pilote ne le dit pas. Indiquez une valeur en Mo pour laisser de la place au jeu ou à un autre programme lancé en même temps. |
| **Garder les autres onglets sur la carte graphique** | Activé (par défaut) : le changement d'onglet est instantané tant qu'il y a de la place. Désactivé : seule la map affichée reste sur la carte graphique, les autres se rechargent quand vous les affichez. |
| **Libérer la mémoire graphique maintenant** | Libère tout de suite les maps des autres onglets, les textures et modèles inutilisés, et les miniatures. |

La ligne sous ces paramètres montre la mémoire utilisée et sa source (« mesurée par le pilote » ou « estimée »).

Les mêmes paramètres sont enregistrés sous `vram_budget_mb` et `keep_background_maps_on_gpu` dans la table `[render]` de `settings.toml`. Par MCP : `set_setting` avec `render.vram_budget_mb`, `get_gpu_memory` pour les mesures, `free_gpu_memory` pour libérer de la mémoire tout de suite. Les scripts peuvent afficher les mesures avec la commande `vram`.

### Quand l'avertissement revient sans cesse

- Fermez les onglets que vous n'utilisez pas : chaque map ouverte coûte de la mémoire graphique tant qu'elle est sur la carte.
- Désactivez **Garder les autres onglets sur la carte graphique**.
- Fermez le jeu ou les autres programmes 3D quand vous éditez une très grande map.
- Baissez le budget si un autre programme a besoin de la carte graphique en même temps.
- Un énorme dossier de contenu monté ne coûte pas de mémoire graphique en lui-même : seuls les textures et modèles des maps ouvertes sont chargés.

## Mémoire de l'ordinateur (RAM)

Hammer Pro garde une petite réserve de mémoire. Quand l'ordinateur n'a plus de mémoire, la réserve est rendue pour que Hammer Pro puisse continuer, vos maps modifiées sont sauvegardées tout de suite dans le dossier des sauvegardes automatiques, et un message vous demande de fermer d'autres programmes ou des onglets. La réserve est reprise 30 secondes plus tard s'il y a de la place.

Si une allocation échoue quand même (une seule demande plus grande que toute la mémoire libre), Hammer Pro écrit un rapport de plantage (« memory allocation of N bytes failed ») et, quand l'échec a eu lieu dans une tâche de fond, sauvegarde les maps modifiées avant de se fermer. Voir [Plantages et journaux](crashes-and-logs.md).

## Pages liées

- [Périphérique graphique perdu](graphics-device-lost.md)
- [Plantages et journaux](crashes-and-logs.md)
- [Performances de l'éditeur](performance.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
