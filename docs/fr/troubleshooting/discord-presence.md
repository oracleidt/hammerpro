[Documentation](../README.md) > Dépannage > Le statut Discord ne s'affiche pas | [English](../../en/troubleshooting/discord-presence.md)

# Le statut Discord ne s'affiche pas

Hammer Pro peut afficher ce que vous mappez dans votre profil Discord (la map, l'outil actif, le jeu et le temps passé sur la map), comme Hammer++. Si rien n'apparaît, suivez cette liste. La fonction elle-même est décrite dans [Statut Discord (Rich Presence)](../features/discord.md).

## Liste de vérification

1. **Lisez l'état.** **Paramètres > Discord** affiche une ligne **État :** : *Connecté*, *Connexion à Discord...*, *Discord n'est pas lancé*, *Discord a refusé la connexion*, *Statut désactivé*, *ID d'application invalide*. La signification de chacun est dans [Statut Discord (Rich Presence)](../features/discord.md#état). Les assistants IA lisent la même chose avec l'outil MCP `get_discord_status`.
2. **Afficher l'activité sur Discord est activé** (**Paramètres > Discord**, activé par défaut). Le désactiver efface l'activité aussitôt.
3. **L'ID d'application.** Laissez **ID de l'application Discord** vide : Hammer Pro utilise sa propre application Discord. Un ID personnalisé doit être un nombre de 17 à 20 chiffres pris sur la page **General Information** de votre application ; avec un ID erroné l'état indique *Discord a refusé la connexion (Invalid Client ID (4000))*.
4. **L'application Discord pour ordinateur tourne sur ce PC.** Discord dans un navigateur ne peut pas recevoir le statut : Hammer Pro dialogue avec le client Discord local par ses canaux IPC (`\\.\pipe\discord-ipc-0` à `discord-ipc-9` sous Windows).
5. **Le partage d'activité est autorisé dans Discord.** Dans les paramètres de Discord, **Confidentialité des activités**, activez **Partager vos activités détectées**.
6. **Patientez un peu.** Les mises à jour partent au plus toutes les 15 secondes (une nouvelle map s'affiche aussitôt). Quand Discord n'est pas lancé, Hammer Pro réessaie discrètement, d'abord après 5 secondes, puis moins souvent (jusqu'à toutes les 20 secondes) : lancer Discord après Hammer Pro fonctionne, cela prend jusqu'à 20 secondes.

Les lancements automatisés (`--script`, tests) ne contactent jamais Discord (état *Désactivé pendant les exécutions automatisées*).

## Utiliser votre propre application Discord

Seulement pour afficher un autre nom ou d'autres images que ceux de Hammer Pro :

1. Ouvrez le Discord Developer Portal et créez une application (c'est son nom qu'affiche votre profil).
2. Dans **Rich Presence > Art Assets**, envoyez une grande image avec la clé `hammerpro` (sans elle, l'activité n'a pas d'image).
3. Sur la page **General Information**, copiez l'**Application ID**.
4. Dans Hammer Pro, **Outils > Options...** (`Ctrl+,`), page **Discord**, collez-le dans **ID de l'application Discord**. Videz le champ pour revenir à l'application de Hammer Pro.

## La page Discord des paramètres

| Option | Effet | Défaut |
|---|---|---|
| **Afficher l'activité sur Discord** | Active ou coupe le statut. | Activé |
| **ID de l'application Discord** | Vide : l'application de Hammer Pro. Sinon l'ID de votre propre application. | Vide |
| **Afficher le nom de la map** | Désactivé : « Édite une map » au lieu du nom de la map. | Activé |
| **Afficher l'outil actif** | Désactivé : le jeu cible est affiché à la place (et « Compilation... » pendant une compilation). | Activé |
| **Afficher le temps écoulé** | Le temps passé sur la map. | Activé |
| **Lien du bouton « Obtenir Hammer Pro »** | Page web facultative (`https://...`) affichée en bouton sous l'activité ; vide : pas de bouton. | Vide |

Ces réglages sont enregistrés dans la table `[discord]` de `settings.toml` (`enabled`, `application_id`, `show_map`, `show_tool`, `show_elapsed`, `project_url`).

## FAQ

**L'activité s'affiche mais sans le logo.** Avec l'application de Hammer Pro, patientez quelques minutes : Discord peut être lent à envoyer les images. Avec votre propre application, la clé de l'image doit être exactement `hammerpro`.

**Les autres ne voient pas le nom de la map.** Vérifiez **Afficher le nom de la map**. Désactivé, Discord affiche « Édite une map ».

**Ça marchait, puis plus rien après un redémarrage de Discord.** Hammer Pro se reconnecte tout seul en moins de 20 secondes ; l'état affiche *Discord n'est pas lancé* entre-temps.

## Pages liées

- [Statut Discord (Rich Presence)](../features/discord.md)
- [Fenêtre des paramètres](../features/settings-dialog.md)
- [Référence des paramètres](../reference/settings.md)
- [FAQ](faq.md)

---
[Retour à l'index de la documentation](../README.md)
