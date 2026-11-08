[Documentation](../README.md) > Fonctionnalités > Éditeur de matériaux | [English](../../en/features/material-editor.md) | [Русский](../../ru/features/material-editor.md)

# Éditeur de matériaux

La fenêtre **Éditeur de matériaux** modifie les fichiers VMT sans quitter Hammer Pro. Elle a deux modes : **Un matériau**, un éditeur complet façon VMT Editor (shader, paramètres par page avec de vrais contrôles, aperçu et texte du VMT côte à côte), et **Plusieurs matériaux**, un éditeur par lot qui définit ou retire les mêmes paramètres dans toute une liste de matériaux d'un coup. Les modifications gardent les commentaires et la mise en forme des fichiers, et chaque fichier sur le disque reçoit une sauvegarde `.vmt.bak` avant sa première modification.

## Ouvrir l'éditeur

- **Modifier le matériau** dans l'en-tête du [navigateur de textures](texture-browser.md) : ouvre l'éditeur sur le matériau actuel, ou sur la sélection multiple du navigateur (`Ctrl+clic`, `Maj+clic`).
- Clic droit sur une vignette > **Modifier le matériau...** : l'ouvre sur ce matériau (ou sur la sélection multiple si le matériau en fait partie).
- [Outil texture](texture-tool.md) : **Modifier le matériau** à côté de **Parcourir...** l'ouvre sur le matériau des faces sélectionnées.
- `Échap` ou le bouton de fermeture de la fenêtre la ferme.

## Charger des matériaux

La ligne du haut remplit la liste des matériaux sur lesquels la fenêtre travaille :

| Commande | Effet |
|---|---|
| **Charger :** **Matériau actuel** | Le matériau actuel du navigateur de textures. |
| **Sélection du navigateur (n)** | Les matériaux sélectionnés avec `Ctrl+clic` ou `Maj+clic` dans le navigateur de textures. |
| **Utilisés par la sélection** | Les matériaux des faces et objets sélectionnés. |
| **Ajouter un matériau par son nom** + **Ajouter** | Ajoute un matériau (aussi `Entrée`). |
| **Recharger** | Relit les fichiers. |
| **Effacer** | Vide la liste. |

Choisissez ensuite le mode : **Un matériau** (par défaut) ou **Plusieurs matériaux**.

Chaque matériau a un emplacement :

| Emplacement | Signification |
|---|---|
| **Fichier** | Un fichier VMT sur le disque (survolez pour son chemin) : écrit sur place. |
| **Image** | Généré à partir d'une image d'un dossier personnalisé : enregistrer crée un vrai fichier VMT à côté de l'image. |
| **Archive** | Dans un VPK ou un GMA (lecture seule) : il doit être copié dans un dossier personnalisé pour être modifié. |
| **Introuvable** | Absent du contenu chargé. |

## Un matériau

| Commande | Effet |
|---|---|
| `◀`, liste des matériaux, `▶` | Avec plusieurs matériaux chargés, choisit celui qu'on modifie. Changer avec des modifications non enregistrées est refusé : **Enregistrez ou annulez d'abord les modifications de ce matériau**. |
| Étiquette d'emplacement | Voir plus haut. Pour un matériau archivé, **Copier dans :** choisit le dossier personnalisé qui reçoit la copie. |
| **Modifié** | Affiché quand le texte diffère du fichier. |
| **Enregistrer** | Écrit le VMT (l'original est gardé en `.vmt.bak`). Un matériau archivé est écrit en copie dans le dossier personnalisé choisi ; un matériau introuvable y est créé. |
| **Annuler les modifications** | Revient au texte du fichier. |
| **Shader** | Change le shader (les paramètres restent) : LightmappedGeneric, VertexLitGeneric, UnlitGeneric, UnlitTwoTexture, WorldVertexTransition, Lightmapped_4WayBlend, Water, Refract, Sprite, SpriteCard, DecalModulate, Cloud, Sky, Patch, Wireframe. |
| **Tous les paramètres** | Montre aussi les paramètres que lisent les autres shaders. Désactivé par défaut : seulement les paramètres de la famille du shader actuel. |
| **Chercher un paramètre** | Filtre les paramètres de la page par clé ou par description. |

Les matériaux qui utilisent le shader `Water_DX9_HDR` sont aussi reconnus comme de l’eau.

La colonne de gauche liste les pages : **Texture de base**, **Transparence**, **Relief (bump)**, **Détail**, **Reflets**, **Phong**, **Lumière de contour**, **Auto-illumination**, **Éclairage et surface**, **Balancement (arbres)**, **Eau**, **Réfraction**, **Sprite**, **Options de compilation**, **Autres paramètres**. Les pages qui ont des paramètres définis affichent leur nombre en gras ; les pages sans rien pour ce shader sont masquées.

Avec Lightmapped_4WayBlend, **Texture de base** liste aussi les paramètres du mélange à 4 textures : `$basetexture3`, `$basetexture4`, `$textureN_lumstart` et `$textureN_lumend`, `$textureN_blendstart` et `$textureN_blendend`, `$lumblendfactorN`, `$textureN_uvscale` et `$textureN_blendmode` ; **Relief (bump)** liste `$textureN_bumpblendfactor`. Les vues 3D les suivent (voir [Mélanges à 4 textures](displacement-tool.md#mélanges-à-4-textures-lightmapped_4wayblend)).

Chaque ligne de paramètre montre sa clé (en gras si elle est définie), un contrôle, un bouton `✖` (**Retirer (retour à la valeur par défaut)**) quand elle est définie, et une ligne qui explique son rôle. Le contrôle dépend du paramètre :

| Type de paramètre | Contrôle |
|---|---|
| Activé ou non | Une case à cocher (décochée, la clé est retirée). |
| Nombre | Un curseur pour les petites plages, sinon un champ à glisser. |
| Couleur | Un champ texte (`[1 1 1]`, on peut saisir des valeurs au-dessus de 1) et un sélecteur de couleur. |
| Choix | Une liste de valeurs nommées. |
| Propriété de surface | Une liste des valeurs courantes de `$surfaceprop`. |
| Texture | Un champ de chemin (`dossier/texture`) et un bouton `…` (**Choisir un fichier de texture**) : le fichier (VTF, PNG, TGA, JPG) doit se trouver dans un dossier `materials`, son chemin est écrit sans extension. |
| Texte ou vecteur | Un champ texte qui affiche la valeur par défaut. |

**Autres paramètres** liste les paramètres du fichier que les pages ne connaissent pas, chacun modifiable avec un bouton pour le retirer, plus une ligne pour ajouter n'importe quel paramètre (`$parameter`, **valeur**, **Ajouter**). Les blocs comme `Proxies` se modifient dans le texte.

À droite, un aperçu du matériau enregistré (**Aperçu du matériau enregistré**) et le **Texte du VMT**, modifiable : les contrôles et le texte changent ensemble. Si le texte n'est pas un VMT valide, la fenêtre indique **Le texte n'est pas un VMT valide : corrigez-le ci-dessous** et n'affiche plus que le texte.

## Plusieurs matériaux

Le mode par lot affiche un tableau des matériaux chargés (**Matériau**, **Emplacement**, **Shader**, un bouton pour retirer une ligne) et les **Modifications** à faire dans tous :

| Paramètre | Choix |
|---|---|
| **Texture de base** (`$basetexture`), **Carte de normales** (`$bumpmap`), **Carte d'environnement** (`$envmap`), **Mots-clés** (`%keywords`) | **Conserver**, **Définir** (avec une valeur), **Supprimer** |
| **Propriété de surface** (`$surfaceprop`) | **Conserver**, **Définir** (champ texte et liste des valeurs courantes), **Supprimer** |
| **Teinte de la carte d'environnement** (`$envmaptint`) | **Conserver**, **Définir** (`[1 1 1]`), **Supprimer** |
| **Translucide** (`$translucent`), **Test alpha** (`$alphatest`), **Double face** (`$nocull`), **Auto-illumination** (`$selfillum`) | **Conserver**, **Activé**, **Désactivé** |

Chaque ligne commence sur **Conserver**. **Autres paramètres** et **+ Ajouter un paramètre** ajoutent des lignes avec n'importe quelle clé.

Le pied de fenêtre affiche **Fichiers : N, issus d'images : N, dans des archives : N, introuvables : N**. Avec des matériaux archivés, **Copier les matériaux archivés dans** et un dossier personnalisé les copient avant modification (une copie hors du préfixe du dossier reçoit un nouveau nom : **... sera copié sous le nom ...**). **Appliquer à N matériau(x)** écrit tous les fichiers en arrière-plan (**Écriture des fichiers...**), puis indique **N matériau(x) mis à jour** (les erreurs vont dans la console). Le bouton reste grisé tant qu'aucune ligne n'est différente de **Conserver** (**Choisissez au moins une modification**).

## Astuces

- Pour rendre un groupe de matériaux double face, sélectionnez-les dans le navigateur avec `Ctrl+clic`, ouvrez **Modifier le matériau**, passez en **Plusieurs matériaux** et mettez **Double face** sur **Activé**.
- **Utilisés par la sélection** charge tous les matériaux d'une pièce en un clic.
- La sauvegarde `.vmt.bak` n'est faite qu'une fois, avant la première modification : elle garde le fichier d'origine.

## FAQ

**Pourquoi ne puis-je pas modifier un matériau du jeu ?** Il se trouve dans une archive VPK ou GMA. Choisissez un dossier personnalisé (**Copier dans :** en mode **Un matériau**, **Copier les matériaux archivés dans** en mode **Plusieurs matériaux**) : l'enregistrement y écrit la copie modifiée.

**Je n'ai aucun dossier personnalisé.** La fenêtre indique **Ajoutez d'abord un dossier personnalisé dans le navigateur de textures**. Voir [Dossiers de textures personnalisés](custom-texture-folders.md).

**Mes commentaires sont-ils conservés ?** Oui, les paramètres sont modifiés dans le texte existant ; commentaires et mise en forme restent.

**La vue 3D se met-elle à jour ?** Oui, le matériau est rechargé après l'enregistrement.

## Pages liées

- [Navigateur de textures](texture-browser.md)
- [Dossiers de textures personnalisés](custom-texture-folders.md)
- [Éditeur UV](uv-editor.md)
- [Outil de texture](texture-tool.md)
- [Référence des outils MCP](../../MCP.md) (en anglais : `read_material`, `edit_material`, `get_material_info`, `get_material_preview`)

---
[Retour à l'index de la documentation](../README.md)

Les matériaux WorldVertexTransition acceptent deux textures de base de résolutions différentes. Le chargement suit la plage de mipmaps de Source et s’arrête dès qu’une dimension atteint 32 pixels ; NOMIP conserve un seul niveau. Les derniers niveaux inutilisés ne teintent ainsi plus les matériaux blend personnalisés à distance. L’éditeur VTF conserve toute la chaîne de mipmaps du fichier.
