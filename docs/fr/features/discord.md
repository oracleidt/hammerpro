[Documentation](../README.md) > Fonctionnalités > Statut Discord | [English](../../en/features/discord.md)

# Statut Discord (Rich Presence)

Comme Hammer++, Hammer Pro peut afficher ce que vous mappez dans votre profil Discord : la map, l'outil actif, le jeu et le temps passé sur la map.

## Où le régler

**Paramètres > Discord** (**Outils > Options...**, `Ctrl+,`), section **Statut Discord (Rich Presence)**. Les réglages sont enregistrés dans `[discord]` de settings.toml.

## Options

| Option | Effet | Défaut |
| --- | --- | --- |
| **Afficher l'activité sur Discord** | Active le statut. Le désactiver efface l'activité aussitôt. Clé `enabled`. | activé |
| **Afficher le nom de la map** | Affiche *Édite gm_city.vmf* ; désactivé, *Édite une map*. Clé `show_map`. | activé |
| **Afficher le temps écoulé** | Affiche le temps passé sur la map. Clé `show_elapsed`. | activé |
| **Afficher l'outil actif** | Affiche l'outil utilisé. Désactivé : le jeu cible est affiché à la place (et *Compilation...* pendant une compilation). Clé `show_tool`. | activé |
| **ID de l'application Discord** | Facultatif : l'ID de votre propre application Discord, pour remplacer celle de Hammer Pro (section **Application Discord**). Vide : l'application de Hammer Pro est utilisée. Clé `application_id`. | vide (application de Hammer Pro) |
| **Lien du bouton « Obtenir Hammer Pro »** | Page web facultative (`https://...`) affichée en bouton **Obtenir Hammer Pro** sous l'activité ; vide : pas de bouton. Clé `project_url`. | vide |

## Ce que Discord affiche

| Ligne | Contenu |
| --- | --- |
| Détails | *Édite map.vmf*, *Édite une map* (nom masqué) ou *Aucune map ouverte*. |
| État | L'outil actif, ou *Compilation...* pendant une compilation, ou le nom du jeu. |
| Durée | Le temps depuis l'ouverture de la map (avec **Afficher le temps écoulé**). |
| Grande image | Le logo Hammer Pro (clé d'image `hammerpro`), avec « Hammer Pro » et la version. |
| Petite image | L'icône du jeu de la map, avec son nom. |

## L'application

Hammer Pro est livré avec sa propre application Discord, nommée « Hammer Pro », avec le logo sous la clé d'image `hammerpro` : il n'y a rien à configurer. Le champ **ID de l'application Discord** est vide et affiche *Hammer Pro (intégré)*.

Pour afficher l'activité sous un autre nom ou avec d'autres images, créez votre propre application sur le Discord Developer Portal, envoyez vos images dans **Rich Presence > Art Assets** (clé `hammerpro` pour la grande image ; icônes de jeux avec les clés `gmod`, `hl2`, `hls`, `hl2dm`, `tf2`, `css`, `dods`, `portal`, `portal2`, `l4d`, `l4d2`, `csgo`, les autres jeux utilisent leur propre id de jeu), puis collez son **Application ID** (page General Information) dans le champ. L'ID est un nombre de 17 à 20 chiffres ; toute autre valeur est refusée avec un message. Videz le champ pour revenir à l'application de Hammer Pro.

## État

La ligne **État :** sous les options indique ce que fait le statut, et se met à jour en moins d'une seconde :

| État | Signification |
| --- | --- |
| *Connecté : votre activité est affichée sur Discord* | Hammer Pro dialogue avec l'application Discord et l'activité est envoyée. |
| *Connexion à Discord...* | Première connexion, ou une option vient de changer. |
| *Discord n'est pas lancé (nouvel essai en arrière-plan)* | Aucune application Discord de bureau n'a répondu sur cet ordinateur. Hammer Pro réessaie après 5 secondes, puis moins souvent (toutes les 20 secondes au plus) : lancez Discord et l'activité apparaît d'elle-même. |
| *Discord a refusé la connexion (raison)* | Discord a répondu mais refuse l'application, par exemple *Invalid Client ID (4000)* avec un ID personnalisé erroné. |
| *Statut désactivé* | **Afficher l'activité sur Discord** est désactivé. |
| *ID d'application invalide : statut désactivé* | Le champ contient autre chose qu'un ID d'application. |
| *Aucun ID d'application : statut désactivé* | Aucun ID à utiliser (seulement dans les versions sans l'application intégrée). |
| *Désactivé pendant les exécutions automatisées* | `--script`, tests : le statut ne démarre jamais. |

## Prérequis

L'application Discord de bureau doit tourner sur le même ordinateur, avec **Partager vos activités détectées** activé dans les paramètres **Confidentialité des activités** de Discord. Hammer Pro lui parle directement par la connexion locale de Discord, sur un thread de fond : quand Discord n'est pas lancé, il réessaie discrètement et l'éditeur ne l'attend jamais. Les sessions automatisées (`--script`, tests) n'atteignent jamais votre Discord.

## Astuces

- Désactivez **Afficher le nom de la map** quand vous travaillez sur un projet secret ; le statut indique alors juste *Édite une map*.
- Partagez un lien vers la page Workshop de votre map ou votre communauté avec le lien du bouton.

## FAQ

**Rien ne s'affiche sur Discord.** Regardez la ligne **État :** dans **Paramètres > Discord**, vérifiez que l'application Discord de bureau tourne et que le partage d'activité est activé. Voir [Le statut Discord ne s'affiche pas](../troubleshooting/discord-presence.md).

**Le statut ralentit-il l'éditeur ?** Non, il tourne sur son propre thread et les mises à jour sont espacées.

**Ça marche avec Discord dans le navigateur ?** Non, le statut nécessite l'application de bureau sur le même ordinateur.

**Faut-il créer une application Discord ?** Non, Hammer Pro a la sienne. Un ID personnalisé sert seulement à afficher un autre nom ou d'autres images.

## MCP

Les assistants IA lisent l'état avec `get_discord_status` (état, son texte, l'ID d'application utilisé, les lignes détails et état envoyées) et changent les options avec `set_setting` sur les clés `discord.*`. Voir la [référence des outils MCP](../../MCP.md).

## Pages liées

- [Le statut Discord ne s'affiche pas](../troubleshooting/discord-presence.md)
- [Fenêtre des paramètres](settings-dialog.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
