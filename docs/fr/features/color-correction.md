[Documentation](../README.md) > Fonctionnalités > Correction des couleurs | [English](../../en/features/color-correction.md) | [Русский](../../ru/features/color-correction.md)

# Correction des couleurs

Hammer Pro crée les tables de correspondance de couleurs que Garry's Mod utilise pour les entités `color_correction`, et prévisualise ces entités dans les vues 3D comme le jeu les applique.

Deux outils :

- l'**éditeur de correction des couleurs**, qui construit et modifie une table avec des images avant et après en direct, et peut l'afficher dans les vues 3D ;
- la **prévisualisation des color_correction**, qui applique les tables nommées par les entités `color_correction` et `color_correction_volume` de la map selon la distance de la caméra.

## Le format de fichier et son emplacement

Une table de correction est un fichier `.raw` d'exactement 98304 octets : un cube de 32 x 32 x 32 octets RVB (32 x 32 x 32 x 3), le rouge variant le plus vite, puis le vert, puis le bleu. La table identité laisse toutes les couleurs inchangées.

Une entité `color_correction` nomme sa table dans la clé **Lookup Filename** (`filename`). Le moteur lit ce chemin par son système de fichiers, relatif au dossier du jeu ou à un addon : la valeur de la clé doit donc être le chemin du fichier tel qu'il est livré. L'éditeur propose trois dossiers :

| Dossier | Remarques |
|---|---|
| `scripts/colorcorrection` (par défaut) | `scripts/colorcorrection/cc_nom.raw`. |
| `materials/colorcorrection` | Même mécanisme, autre dossier. |
| `materials/correction` | La convention du Source SDK (`materials/correction/cc_nom.raw`). |

Quel que soit le dossier, l'éditeur montre le `Lookup Filename` exact à saisir, avec un bouton **Copier**. Si vous publiez l'addon sur le Workshop, vérifiez que le dossier choisi est autorisé par la liste blanche de l'outil d'empaquetage d'addons de Garry's Mod.

## Ouvrir l'éditeur

- **Outils > Contenu et éditeurs > Éditeur de correction des couleurs...** (action `color_correction_editor`) ouvre la fenêtre, et la ferme au second appel.
- MCP : `open_media_editor` avec `editor` à `color` (et un `path` pour charger un `.raw`).

## L'éditeur

La colonne de gauche contient les réglages, celle de droite l'aperçu et l'enregistrement.

| Section | Paramètres |
|---|---|
| **Préréglages** | Neutre, Noir et blanc, Sépia, Chaud, Froid, Fort contraste, Délavé, Nuit, Éclatant, Cinéma, Horreur, Inversé. Un préréglage remplace les réglages en cours. |
| **Base** | Exposition (EV, en lumière linéaire), Luminosité, Contraste, Gamma, Saturation, Vibrance (renforce davantage les couleurs ternes), Teinte (degrés), Inverser. |
| **Équilibre des couleurs** | Température (froid à chaud), Vert / magenta, Ombres et Hautes lumières (relèvent ou baissent seulement la partie sombre ou la partie claire). |
| **Teinte et colorisation** | La teinte multiplie les couleurs par une couleur ; la colorisation remplace la teinte par une couleur en gardant la luminosité (sépia, bichromie). Chacune a une force. |
| **Niveaux** | Noir et blanc d'entrée, tons moyens (gamma), noir et blanc de sortie, pour le maître RVB et pour les canaux rouge, vert et bleu. |
| **Courbes** | Une courbe lisse par canal (maître RVB, rouge, vert, bleu). Cliquez pour ajouter un point, glissez un point pour le déplacer, clic droit sur un point pour le supprimer. |

Clic droit sur un curseur pour le réinitialiser. L'ordre des opérations est : exposition, température et balance de teinte, luminosité, contraste, ombres et hautes lumières, niveaux, courbes, gamma, teinte, saturation et vibrance, teinte multiplicative, colorisation, inversion.

### Aperçu

**Avant** et **Après** montrent une mire intégrée (ciel, barre de teintes, dégradé de gris, tons chair, pastilles de couleur) ou n'importe quelle image chargée avec **Utiliser une image...**. La ligne sous les images indique de combien la table déplace les couleurs.

**Aperçu dans les vues 3D** démarre automatiquement à l’ouverture et suit les réglages en direct. Sélectionnez une seule entité `color_correction` ou `color_correction_volume` avant d’ouvrir l’éditeur pour appliquer le brouillon à travers cette entité : sa décroissance avec la distance, son poids maximal, son état et les limites de son volume restent pris en compte. Les autres corrections conservent leurs tables. Sans correction sélectionnée, le brouillon s’applique globalement à poids plein. Fermer la fenêtre ou décocher l’option rétablit l’aperçu de la map.

### Importer un .raw

**Importer un .raw...** choisit un fichier ; le champ à côté charge un chemin du jeu (`scripts/colorcorrection/cc_nom.raw`). La table importée devient la base et chaque correction s'applique par-dessus, ce qui permet d'affiner une table existante. **Retirer** revient à la base identité.

### Enregistrer

Choisissez le **Dossier du jeu ou de l'addon** (la racine, avec **Parcourir...**), le **Sous-dossier** dessous et le **Nom**, puis **Enregistrer le .raw**. La racine est par défaut le dossier au-dessus du dossier `maps` de la map enregistrée, sinon le dossier du jeu. Un fichier existant n'est remplacé que si vous cochez **Remplacer le fichier existant**.

## Prévisualiser les entités color_correction dans les vues 3D

Le plus rapide est le mode de rendu **3D Correction colorimétrique** (**Affichage > Mode de rendu 3D**, `Alt+9`, la liste des modes de la barre d'outils, MCP `set_render_mode` avec `color_corrected`) : la vue texturée avec les tables des entités de la map appliquées, sans activer le réglage ci-dessous. C'est le mode texturé plus l'aperçu, donc il suit les mêmes règles.

**Vue > Options d'affichage > Prévisualiser les entités color_correction** (action `toggle_color_correction_preview`, réglage `lab.color_correction_preview`, désactivé par défaut) applique les tables des entités de la map aux vues 3D, depuis la caméra de l'éditeur ou, dans la [vue joueur](player-view.md), depuis la caméra du joueur. Il suit le jeu :

| Clé | Effet |
|---|---|
| `filename` | La table. Une entité sans table lisible ne fait rien (l'outil MCP indique `no_file` ou `file_not_found`). |
| `minfalloff`, `maxfalloff` | Poids plein jusqu'à `minfalloff` unités, décroissant jusqu'à 0 à `maxfalloff`. Si l'une vaut -1, ou si elles sont égales, le poids ne dépend pas de la distance. |
| `maxweight` | Le poids est multiplié par cette valeur. |
| `fadeInDuration`, `fadeOutDuration` | Secondes pour apparaître quand l'entité devient active et pour disparaître quand elle s'arrête. |
| `exclusive` | Tant qu'une entité exclusive a du poids, les non exclusives sont ignorées. |
| `StartDisabled` | L'entité démarre désactivée (les entrées `Enable` et `Disable` n'existent que dans le jeu en cours). |

Plusieurs entités se mélangent : leurs poids s'additionnent (réduits ensemble quand le total dépasse 1) et le reste est l'image inchangée. Les entités `color_correction_volume` (`filename`, `maxweight`, `fadeDuration`, `StartDisabled`) s'appliquent quand la caméra est dans leurs brushes.

La correspondance est faite par la passe 3D elle-même : l'image finale est convertie en couleurs d'affichage, cherchée dans la table avec un filtrage trilinéaire puis reconvertie. La correction s'applique dans tous les modes d'ombrage et reste distincte de la correction de couleurs Hammer++ de toute la map des réglages d'éclairage, qui suit toujours l'affichage de l'éclairage précalculé.

## MCP

| Outil | Usage |
|---|---|
| `color_correction_presets` | Les préréglages, les paramètres avec leurs plages, le format de fichier et la convention de chemin. |
| `color_correction_generate` | Construit une table à partir d'un `preset` ou d'un `grade` (et d'une table `base` facultative) et l'enregistre en `root/folder/name.raw` ; renvoie la valeur exacte de la clé `filename`. `dry_run` renseigne sans écrire. |
| `color_correction_inspect` | Vérifie un `.raw` (taille, identité, couleur moyenne, écart aux couleurs). |
| `color_correction_preview` | L'« Aperçu dans les vues 3D » de l'éditeur : un préréglage, un grade ou un `.raw`, jusqu'à `off`. |
| `color_correction_zones` | Chaque entité color_correction avec son fichier, sa distance, sa décroissance, son poids et son état. |
| `open_media_editor` | Ouvre la fenêtre. |

L'interrupteur est aussi accessible par `set_setting` (`lab.color_correction_preview`) et `run_action` (`toggle_color_correction_preview`).

## Voir aussi

- [Éditeur de sons](sound-editor.md)
- [Aperçu de l'éclairage](lighting-preview.md)
- [Vue joueur](player-view.md)
