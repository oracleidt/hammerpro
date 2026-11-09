[Documentation](../README.md) > Team Mapping | [English](../../en/features/team-mapping.md) | [Русский](../../ru/features/team-mapping.md)

# Team Mapping

Team Mapping partage un VMF entre l'éditeur de l'hôte et ses invités. Le menu **Team Mapping** se trouve en haut, juste avant **MCP**.

## Créer une session

1. Ouvre le VMF à partager.
2. Choisis **Relais (recommandé)** pour une connexion automatique protégeant les adresses, ou **Connexion directe** si tu peux fournir une IP et un port UDP accessibles.
3. Donne un nom à la session : de 1 à 48 lettres ASCII, chiffres, espaces, caractères `_` ou `-`.
4. Choisis un pseudo : de 1 à 12 lettres ASCII, chiffres, caractères `_` ou `-`. Deux participants ne peuvent pas utiliser le même pseudo, même avec une casse différente.
5. Définis un mot de passe de 8 à 256 caractères. Il est masqué par défaut ; son affichage demande une action explicite.
6. Choisis la map dans **Map à partager**, puis clique sur **Héberger la map sélectionnée**, attends l'état de connexion, puis sur **Copier l'invitation**.
7. Communique l'invitation, le nom de session et le mot de passe aux participants par un canal de confiance.

L'invitation est un accès privé, pas une annonce publique. Elle contient un secret aléatoire de session et l'identité cryptographique de l'hôte. Ne la publie pas. Le mot de passe reste obligatoire. En mode relais, aucun compte, IP à saisir, port à ouvrir, changement de routeur ou serveur à installer n'est nécessaire.

## Rejoindre

Saisis l'invitation, le nom exact de la session, ton pseudo et le mot de passe. Clique sur **Rejoindre dans un nouvel onglet local**. Ton document précédent reste dans son onglet. La map reçue ne reprend pas le chemin du fichier de l'hôte. Enregistre ta copie avec **Enregistrer sous** avant de compiler si l'éditeur le demande.

Choisis le même mode de connexion que l'hôte. En mode direct, ajoute son `IP:port` ; pour IPv6, utilise `[adresse]:port`. Le port UDP proposé à l'hôte est 41840. Les adresses affichées dans son panneau sont locales et ne sont pas forcément accessibles depuis Internet : une IP publique et une redirection de port sur le routeur ou le pare-feu peuvent être nécessaires. Hammer Pro ne modifie pas automatiquement ces règles. Le mode direct n'utilise ni relais ni découverte publique et ne bascule pas vers un relais.

Les participants voient le nom de session, les pseudos et la latence aller-retour mesurée. Des points colorés indiquent les positions des caméras, avec un repère de direction. Les objets sélectionnés par une autre personne ont un contour de sa couleur. Le bouton **Repérer** oriente ta caméra vers elle.

## Modifications, conflits et annulation

L'hôte valide et ordonne les modifications. Le VMF initial est envoyé à l'arrivée ; ensuite, les messages contiennent les objets modifiés. Les positions et sélections sont envoyées au maximum dix fois par seconde, uniquement lorsqu'elles changent. Le panneau indique les octets transférés et l'état de synchronisation.

Une modification sur un objet sélectionné par une autre personne est refusée. Des changements sur des objets distincts peuvent fusionner. Si ton changement se base sur une ancienne version du même objet, il est refusé plutôt que d'écraser le travail plus récent. Ta version locale reste disponible dans **Récupérer la copie locale (Enregistrer sous)**.

Ctrl+Z et Rétablir concernent tes propres modifications confirmées. Si quelqu'un a modifié l'objet depuis, l'annulation est refusée pour préserver son travail. Attends la synchronisation d'une modification en cours avant d'annuler. Une longue transaction de géométrie peut retarder l'application des changements reçus jusqu'à sa fin.

## Relais et confidentialité des adresses

**Le relais est le choix par défaut.** Dans ce mode, les transports IP directs et la publication d'adresses directes sont désactivés, sans basculement automatique vers une connexion directe. Chaque connexion de session emploie une nouvelle identité réseau ; une ancienne identité directe n'est pas réutilisée. Tous les participants d'une session relais passent par ce relais.

**La connexion directe est un choix explicite.** Elle peut réduire la latence en supprimant un intermédiaire, mais ne masque pas les adresses de l'hôte et de l'invité aux machines connectées. Elle peut être bloquée par le NAT ou le pare-feu. Le chiffrement et l'authentification restent actifs dans les deux modes. Il faut créer une nouvelle session pour changer de mode.

Une capture réseau montre toujours les connexions de ton PC vers les services de relais et de découverte. Le protocole de collaboration ne donne aucune adresse réseau directe aux autres participants. L'opérateur du relais peut voir les adresses des connexions, leur durée et leur volume, mais ne peut pas déchiffrer le contenu des échanges de la map.

Hammer Pro est gratuit et open source. Les relais publics tiers évitent des frais d'hébergement récurrents pour le projet. Les services Iroh/n0 intégrés ne demandent aucune configuration ni compte payant dans Hammer Pro. Leur disponibilité, leur capacité, leur tarification future et leur accessibilité dépendent de leur opérateur. Hammer Pro ne peut pas garantir un service gratuit pour toujours ni une connexion depuis tous les réseaux restrictifs. Le relais peut ajouter de la latence ; le ping affiché est une mesure réelle, pas une promesse de délai. Tu n'as aucun relais dédié à gérer, et le développeur de Hammer Pro n'a pas à en exploiter un.

## Modération de l'hôte

L'hôte peut exclure un participant ou le bannir pour une durée choisie. Zéro signifie jusqu'à la fin de la session. Le motif accompagne le message de refus. La liste des bannissements présente les pseudos, motifs et durées restantes, avec un bouton de débannissement. Les bannissements expirent automatiquement et disparaissent à la fin de la session.

Une exclusion simple autorise une nouvelle connexion avec des accès valides. Le bannissement utilise une identité d'installation prouvée cryptographiquement, propre à la session de l'hôte. La clé locale se trouve dans `team-mapping-identity.key`, dans le dossier des paramètres de Hammer Pro. Aucun numéro de série matériel brut n'est collecté. Ce n'est pas un bannissement matériel inviolable : un client open source modifié ou une identité d'installation réinitialisée peut le contourner. Le mode relais ne révèle volontairement pas les IP des invités ; un bannissement fiable de leur IP réelle est donc impossible pour l'hôte.

**Renouveler les accès et déconnecter les invités** change le secret d'invitation et le mot de passe, puis déconnecte les invités. Les anciennes invitations sont révoquées. Ne communique les nouveaux accès qu'aux personnes à réadmettre. Fermer la session révoque aussi son invitation.

## Snapshots de sécurité

L'hôte reçoit une notification quand un pseudo rejoint la session. Des snapshots locaux compressés sont créés au démarrage de l'hébergement, puis toutes les dix minutes. **Créer un snapshot maintenant** ajoute une copie manuelle. Le dossier conserve au maximum dix fichiers, toutes sessions confondues. La plus ancienne copie est supprimée seulement après l'écriture réussie de la nouvelle. La liste indique le nom de session, la date UTC, la révision et la taille réelle compressée. L'efficacité de la compression dépend du contenu.

Choisis une copie puis confirme **Restaurer** pour remplacer la map chez tous les participants. L'hôte sauvegarde d'abord l'état courant de la map partagée. Si cette copie de sécurité échoue, la restauration est annulée. Les historiques de session sont réinitialisés ; une modification locale non confirmée reste disponible dans la copie de récupération si nécessaire.

Les snapshots sont stockés à côté des autosauvegardes ordinaires, dans `HammerPro/team-mapping-snapshots` sous les données locales de l'application. Ils restent après la fin de session. Les invités ne peuvent ni lire ni restaurer les snapshots de l'hôte. Rien n'est envoyé dans le cloud. Les erreurs de disque sont affichées ; une panne de stockage peut néanmoins empêcher la récupération.

## Compilation, enregistrement et ressources

Celui qui clique sur Compiler utilise uniquement son PC. Son compilateur, ses paramètres, son dossier de travail, son BSP, ses journaux, la copie dans `garrysmod/maps` et le lancement du jeu restent locaux. La compilation prend une copie de la map ; les changements ultérieurs ne modifient pas cette compilation en cours. Un invité ne peut pas ordonner à l'hôte de compiler, d'exécuter une commande, de lancer un programme ou d'écrire un fichier de sortie.

Les textures, modèles, scripts et contenus montés ne sont pas téléchargés ou installés automatiquement par la session. Chaque participant doit disposer des ressources et compilateurs nécessaires. Un contenu manquant peut produire un aperçu différent ou faire échouer la compilation. Les commandes de compilation et chemins locaux de l'hôte ne sont pas partagés.

## Protections et limites actuelles

- Huit participants maximum, hôte compris. L'hôte transmet les changements acceptés à chaque invité ; son débit montant dépend du nombre d'invités et de la géométrie modifiée.
- Les données partagées du VMF sont limitées à 64 MiB et 200 000 unités d'objets. Une modification peut concerner au maximum 8 192 unités et occuper 8 MiB de données sérialisées.
- Les messages, la profondeur des blocs, les tailles de propriétés, les coordonnées, les identifiants et le trafic par participant sont contrôlés. Le réseau expose des modifications de map et des informations de présence, sans accès au MCP ni à des opérations arbitraires sur les fichiers.
- Les chemins de ressources dangereux et les entités de script ou de commande comme `lua_run`, `logic_script`, `point_servercommand` et `point_clientcommand` sont refusés. Adapte les maps qui en dépendent avant d'héberger.
- Les chemins de sauvegarde, paramètres de vue, positions de caméra enregistrées et paramètres de compilation restent locaux.
- Seule la map choisie est partagée. Les autres maps restent locales et la synchronisation continue quand tu changes d'onglet. **Revenir à la map partagée** rouvre le document de session. Fermer son onglet termine la session. Chacun conserve sa copie locale après une déconnexion.
- Pas de transfert d'hôte, de fusion de travail hors ligne, de reconnexion automatique ou de session persistante dans le cloud. Après une coupure, rejoins à nouveau avec des accès valides. Vérifie la synchronisation avant une compilation ou une sauvegarde finale.
- La validation empêche les messages mal formés ou périmés d'écraser le document. Un invité autorisé à modifier peut néanmoins proposer des changements indésirables. Utilise la modération et des sauvegardes.

Voir aussi la [configuration du jeu](../getting-started/game-setup.md) et la [compatibilité VMF](../getting-started/vmf-compatibility.md).

Les caméras et sélections sont actualisées pendant un déplacement. Les changements de géométrie sont envoyés à la validation de l'opération de l'outil. Les modifications reçues continuent à se synchroniser en arrière-plan et sont fusionnées dans le document visible une fois l'opération locale terminée.
