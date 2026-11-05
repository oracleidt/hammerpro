[Documentation](../README.md) > Dépannage > Signaler un bug | [English](../../en/troubleshooting/reporting-bugs.md) | [Русский](../../ru/troubleshooting/reporting-bugs.md)

# Signaler un bug

Les rapports de bug sont les bienvenus. Un bon rapport permet de reproduire le problème rapidement : cette page liste ce qu'il faut y mettre et où le trouver.

## Où signaler

**Aide > Signaler un problème** ouvre la page des problèmes (issues) du projet dans votre navigateur. Vérifiez d'abord si le problème est déjà signalé ; si oui, ajoutez vos détails au rapport existant.

## Ce qu'il faut inclure

| Information | Où la trouver |
|---|---|
| Version de Hammer Pro | **Aide > À propos de Hammer Pro** (version et numéro de compilation). |
| Version de Windows et carte graphique | Le fichier journal indique la carte graphique et l'API utilisées (« graphics adapter: ... »). |
| Ce que vous avez fait, ce que vous attendiez, ce qui s'est passé | Des étapes courtes et numérotées. Précisez l'outil, la commande de menu ou la touche utilisée. |
| La map | Le `.vmf` qui montre le problème, si vous pouvez le partager (ou une petite map faite pour le reproduire). Pour un problème de compilation, joignez aussi le journal de compilation. |
| Le rapport de plantage, après un plantage | Au lancement suivant, la fenêtre **Hammer Pro s'est fermé de façon inattendue** affiche le fichier du rapport : **Copier le rapport** met son texte dans le presse-papiers pour le coller dans votre message, **Afficher le fichier du rapport** ouvre son dossier (`%APPDATA%\HammerPro\crash\`). |
| Le fichier journal | `%LOCALAPPDATA%\HammerPro\logs\hammerpro.log` (lancement en cours) et `hammerpro.old.log` (lancement précédent). Les erreurs des tâches de fond et de la carte graphique s'y trouvent, ainsi que dans le panneau console. |
| Les paramètres, si utile | `%APPDATA%\HammerPro\settings.toml` (il contient des chemins de votre ordinateur ; retirez ce que vous ne voulez pas partager). |

## Conseils

- **Copiez la console.** Le panneau console (`` ` ``) a un bouton qui copie les messages visibles : filtrez d'abord sur les avertissements et les erreurs.
- **Reproduisez avec les réglages par défaut.** Si vous avez changé beaucoup de réglages, réessayez après avoir renommé `settings.toml` (voir [Plantages et journaux](crashes-and-logs.md)). Indiquez dans le rapport si le problème persiste.
- **Problèmes graphiques.** Dites si forcer une autre API avec `WGPU_BACKEND` change quelque chose, et joignez une capture d'écran.
- **Problèmes de compilation.** Indiquez le préréglage (Brouillon, Normal, Final) et le jeu cible utilisés, et si la même map se compile avec les outils de Valve.
- **Éditeur lent.** Joignez les images par seconde (**Paramètres > Général > Afficher les images par seconde**) et les informations de la vue 3D (`O`) à l'endroit où ça rame.
- **Traductions.** Un texte français faux ou manquant est aussi un bug : citez le texte et l'endroit où il apparaît.

## Proposer une correction

Si vous voulez corriger le bug vous-même, voir [Contribuer](../development/contributing.md) et [Compiler depuis les sources](../development/building.md).

## Pages liées

- [Plantages et journaux](crashes-and-logs.md)
- [Erreurs courantes](common-errors.md)
- [FAQ](faq.md)
- [Contribuer](../development/contributing.md)

---
[Retour à l'index de la documentation](../README.md)
