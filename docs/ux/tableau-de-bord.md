# Tableau de bord : des tuiles qui disent l'état, des gestes qui attendent l'appareil

Recherche du 2 octobre 2026, pour la réécriture de l'interface en SvelteKit. Chaque règle renvoie
à une source ouverte ce jour-là. La mise en page n'est pas rediscutée ici : Maison reprend telle
quelle celle d'Ariane (`ariane/docs/ux/mise-en-page.md` : mêmes tailles d'écran, un seul conteneur
de page, échelle de 4, typographie Material 3, colonnes sans trous, en-tête collant dès 600 px,
barre du bas en dessous) et ses jetons (`ariane/web/src/styles/tokens.css` : palette synthe.se,
accent pétrole, Fraunces et Cal Sans). Ce document ne traite que de ce qui est propre à une maison
pilotée à distance.

Contexte Maison : deux personnes, surtout sur téléphone, parfois un portable ou le 32 pouces.
Le serveur interroge les appareils ; une commande est confirmée entre ~100 ms (lampe Zigbee) et
plusieurs dizaines de secondes (TV en veille profonde ~20 s, volet ~30 s de course). Certains
appareils ne disent jamais leur état (la clim Mitsubishi en infrarouge, la TV quand JointSPACE est
tombé). Mesures relevées dans le code actuel (`frontend/src`, React) : rafraîchissement de 3 s
(Hue) à 120 s (Nabaztag), 1 s pour un volet en mouvement ; aucune annonce d'état aux lecteurs
d'écran ; l'erreur n'existe que dans un toast.

## 1. La tuile : nom, état en mots, un seul geste

### Ce que font les références

- **Apple Maison** : « tap an accessory's icon on the left side of the tile […] to quickly turn the
  accessory on or off. Tap the accessory's name on the right side of the tile to show the
  accessory's control » ([Apple, contrôler les accessoires](https://support.apple.com/guide/iphone/control-accessories-iph0a717a8fd/ios)).
  Un appareil muet affiche « No Response » sur sa tuile ([Apple, accessoire qui ne répond pas](https://support.apple.com/en-us/102056)).
- **Home Assistant, carte tuile** : icône, nom et état ; toucher la carte ouvre le détail
  (« more-info »), toucher l'icône bascule l'entité ; des « features » ajoutent sous la tuile un
  seul contrôle du type de l'appareil ([HA, tile card](https://www.home-assistant.io/dashboards/tile/)) :
  ouvrir/arrêter/fermer ou position pour un volet, luminosité ou température de couleur pour une
  lampe, volume pour un lecteur ([HA, features](https://www.home-assistant.io/dashboards/features/)).
- **Google Home** : « Tap the tile to take a quick action », « Touch and hold your device's tile
  until the controls open », « Drag a slider on the tile to gradually adjust »
  ([Google, contrôler ses appareils](https://support.google.com/googlenest/answer/7073578?hl=en)).
- **Material 3, cartes** : une carte peut être une seule grande cible qui ouvre un écran de
  détail ([M3 cards](https://github.com/Glavo/md3-reference-hub/blob/main/docs/components/cards/guidelines.md)).
- **État** : jamais la couleur seule ([WCAG 1.4.1](https://www.w3.org/WAI/WCAG22/Understanding/use-of-color.html)) ;
  Carbon combine forme, couleur et texte, au moins trois éléments, et la couleur seule ne suffit
  jamais ([Carbon, indicateurs d'état](https://carbondesignsystem.com/patterns/status-indicator-pattern/)) ;
  Apple : « Avoid relying solely on different colors to communicate state »
  ([HIG Toggles](https://developer.apple.com/design/human-interface-guidelines/toggles)).

**Consensus** : deux zones, pas plus. À gauche, l'icône est le geste principal (allumer,
éteindre) ; le nom ouvre tout le reste. Au plus un contrôle secondaire visible sur la tuile,
celui qu'on utilise le plus.

### Pour Maison

```
┌──────────────────────────────────────────────┐
│ [◉]  Lampe du salon                    40 %  │  ligne 1 : geste · nom · un fait
│ 48px Allumée                                 │  ligne 2 : l'état en mots
│ ───────────────●──────────────────────────── │  option : un seul contrôle (44 px)
└──────────────────────────────────────────────┘
```

| Appareil | Geste de l'icône | Fait à droite | Contrôle sur la tuile | Dans le détail |
|---|---|---|---|---|
| Lampe Hue, Zigbee | allumer / éteindre | « 40 % » | luminosité | température de couleur, appairage, retrait |
| Prise Meross | allumer / éteindre | « 312 W » | aucun | historique de conso, retrait |
| Volet Matter | aucun (pas de bascule) | « 40 % » | Ouvrir · Arrêter · Fermer | position, étalonnage, retrait |
| TV Philips | allumer / éteindre | « Vol. 18 » | volume − / + | Ambilight, pavé, source |
| Box Android TV | réveiller / endormir | appli en cours | aucun | pavé, applis, APK, appairage |
| Clim (IR) | deux boutons (§ 2) | « 21 °C · Chaud » | aucun | mode, ventilation, ailettes, minuterie |
| Distributeur | aucun | « Servi à 18 h 02 » | « Donner 1 portion » | repas planifiés |
| Fontaine, litière | aucun | niveau, « Propre » | aucun | réglages, compteurs |
| Nabaztag | aucun | « Éveillé » | aucun | — |
| Tempo | aucun | couleur du jour (§ 8) | demain | calendrier, prévisions |

- La tuile est une ligne d'Ariane élargie : `--row-min` (48, 56 sur téléphone), rayon
  `--radius-m`, fond `--surface`, rembourrage `--s-3`. Grille interne : 48 px | 1fr | auto.
- L'icône-bouton fait 48 × 48 px (cible WCAG 2.5.8 ≥ 24, Apple 44 pt, recommandé 44 en 2.5.5,
  [WCAG 2.5.5](https://www.w3.org/WAI/WCAG22/Understanding/target-size-enhanced.html)) ; allumé :
  disque plein `--accent` et icône `--on-accent` ; éteint : cercle vide 1,5 px `--ink-muted`
  (deux formes, contraste 3:1 en clair comme en sombre, [WCAG 1.4.11](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html)).
- La ligne 2 dit l'état en mots : « Allumée », « Éteinte », « Ouvert à 40 % », « Injoignable
  depuis 3 min ». Le fait de droite ne passe jamais à la ligne (`white-space: nowrap`).
- Le nom est un lien vers `/appareils/[id]` : une page de détail, une colonne à `--measure`,
  pas une feuille modale. Raison : les détails de Maison sont longs (clim, distributeur, box) et
  une page se partage, se rafraîchit et garde le bouton Retour.

## 2. Allumer à distance : un bouton pressé, pas un interrupteur

### Ce que disent les sources

- **APG Switch** : `role="switch"`, `aria-checked`, Espace et Entrée ; « it is critical the label
  on a switch does not change when its state changes » ([APG Switch](https://www.w3.org/WAI/ARIA/apg/patterns/switch/)).
- **APG Button** : un bouton bascule porte `aria-pressed` ; même règle d'étiquette fixe ; si
  l'étiquette change avec l'état, c'est un bouton simple ([APG Button](https://www.w3.org/WAI/ARIA/apg/patterns/button/)).
- **Apple** : « Use the switch toggle style only in a list row », et « Outside of a list, use a
  button that behaves like a toggle, not a switch » ([HIG Toggles](https://developer.apple.com/design/human-interface-guidelines/toggles)).
- **NN/g et Material** : un interrupteur prend effet immédiatement, sans Enregistrer
  ([NN/g, toggle switches](https://www.nngroup.com/articles/toggle-switch-guidelines/),
  [M3 switch](https://github.com/Glavo/md3-reference-hub/blob/main/docs/components/switch/guidelines.md)).
- **Home Assistant** fait les deux. Interrupteur optimiste : il bascule tout de suite et, faute
  de nouvel état en 2 s, revient à l'état connu. Et quand l'état est supposé (`assumed_state`)
  ou inconnu, deux boutons « Turn … on » et « Turn … off » à la place de l'interrupteur
  ([ha-entity-toggle.ts](https://github.com/home-assistant/frontend/blob/dev/src/components/entity/ha-entity-toggle.ts),
  [HA, entity](https://developers.home-assistant.io/docs/core/entity/)).
- **UI optimiste** : on montre le résultat attendu pendant l'action ; en cas d'échec, la valeur
  revient à l'ancienne et on affiche l'erreur ([React, useOptimistic](https://react.dev/reference/react/useOptimistic)).
- **Délais** : 0,1 s paraît instantané, 1 s garde le fil de la pensée, au-delà de 10 s
  l'attention part et il faut un indicateur de progression ([NN/g, response times](https://www.nngroup.com/articles/response-times-3-important-limits/)).
  Sous 1 s, pas d'indicateur : il clignote et ralentit le ressenti ; de 1 à 3 s un indicateur
  indéterminé ; au-delà, déterminé ([Primer, loading](https://primer.style/product/ui-patterns/loading/),
  [NN/g, progress indicators](https://www.nngroup.com/articles/progress-indicators/)).
  Le chargement se montre sur le composant qui travaille ([Carbon, loading](https://carbondesignsystem.com/patterns/loading-pattern/)).

### Pour Maison

**Trois cas, trois contrôles.**

1. *État lu sur l'appareil* (Hue, Zigbee, Meross, TV quand JointSPACE répond) : l'icône de la
   tuile est un `<button aria-pressed>` dont le nom accessible est celui de l'appareil
   (« Lampe du salon »), jamais « Allumer ». Hors d'une liste de réglages, donc pas de `switch`
   (Apple).
2. *Réglages dans une page de détail* (mode nuit de la litière, voyant de la fontaine) : une
   ligne de liste avec `role="switch"` et son libellé à gauche (Apple, M3).
3. *État supposé* (clim IR, TV en IR) : deux boutons « Allumer » et « Éteindre », sans
   `aria-pressed`, et sous eux le dernier ordre : « Dernier ordre : allumer, à 21 h 04 ». On ne
   dessine pas un état qu'on ne connaît pas (HA `assumed_state`).

**La commande en vol.** Le geste est optimiste, l'état affiché ne l'est pas :

| Temps écoulé | Bouton | Ligne d'état | Lecteur d'écran |
|---|---|---|---|
| 0 | `aria-pressed` passe à la cible, forme pleine en contour pointillé | inchangée | rien |
| > 1 s | idem + anneau indéterminé autour de l'icône | « Allumage… » | rien |
| état confirmé par l'appareil | forme pleine | « Allumée » | rien (le geste a déjà dit `pressed`) |
| délai dépassé ou erreur | revient à l'état lu | « Pas de réponse · Réessayer » en `--status-down-text` | `role="status"` : « Lampe du salon n'a pas répondu » |

- Délai de confirmation par famille, réglé sur la latence mesurée : lampes 3 s, prises 5 s, TV
  30 s (réveil de veille profonde ~20 s), volets : jusqu'à la fin de course (§ 3).
- Pendant le vol, le bouton reste actif (Primer : pas d'`aria-disabled` pendant un chargement) ;
  un deuxième appui annule l'intention et envoie l'ordre inverse.
- Une commande de plus de 10 s (réveil TV) affiche sa durée attendue : « Réveil de la TV…
  jusqu'à 30 s », barre déterminée sur le temps écoulé (NN/g).
- L'erreur reste sur la tuile tant qu'on n'a pas réessayé ou que l'appareil n'a pas changé ;
  le bandeau global n'est qu'un écho, pas le seul endroit où elle vit.

## 3. Les curseurs : luminosité, volume, position

### Ce que disent les sources

- **APG Slider** : flèches d'un pas, Page haut/bas d'un grand pas (facultatif), Début et Fin
  aux bornes ; `aria-valuetext` quand le nombre seul n'est pas parlant ([APG Slider](https://www.w3.org/WAI/ARIA/apg/patterns/slider/)) ;
  l'exemple de couleur saute dix pas avec Page haut, celui de température donne l'unité dans
  `aria-valuetext` ([APG, color viewer slider](https://www.w3.org/WAI/ARIA/apg/patterns/slider/examples/slider-color-viewer/)).
- **Material 3** : « Changes made with sliders must take effect immediately » ; le nom
  accessible reprend le libellé visible ; pendant le glissement la poignée s'affine et la valeur
  apparaît ([M3 sliders](https://github.com/Glavo/md3-reference-hub/blob/main/docs/components/sliders/guidelines.md),
  [M3 sliders, accessibilité](https://github.com/Glavo/md3-reference-hub/blob/main/docs/components/sliders/accessibility.md)).
  Jetons : poignée 20 px, piste 4 px, couche d'état 40 px ([M3 slider tokens](https://raw.githubusercontent.com/material-components/material-web/main/tokens/versions/v0_192/_md-comp-slider.scss)).
- **Apple** : minimum à gauche, maximum à droite ; retour en direct pendant le geste ; un libellé
  avant le curseur ([HIG Sliders](https://developer.apple.com/design/human-interface-guidelines/sliders)).
- **WCAG 2.5.7** : tout ce qui se fait en glissant se fait aussi d'un seul toucher, par exemple en
  touchant la piste ([Understanding 2.5.7](https://www.w3.org/WAI/WCAG22/Understanding/dragging-movements.html)).
- **Volets** : Home Assistant compte la position en ouverture, « 0 means closed and 100 is fully
  open » ([HA, cover](https://developers.home-assistant.io/docs/core/entity/cover/)). Matter
  compte l'inverse, en centièmes de pour cent de fermeture (`CurrentPositionLiftPercent100ths`,
  [connectedhomeip](https://github.com/project-chip/connectedhomeip/blob/master/src/app/zap-templates/zcl/data-model/chip/window-covering.xml)) ;
  HA retourne la valeur à la frontière, « current position is inverted in matter »
  ([HA, matter/cover.py](https://github.com/home-assistant/core/blob/dev/homeassistant/components/matter/cover.py)).
  Maison fait déjà de même dans `backend/src/matter.rs`.

### Pour Maison

| Curseur | Bornes | Flèche / Page | `aria-valuetext` | Envoi |
|---|---|---|---|---|
| Luminosité | 1 à 100 % | 1 / 10 | « 40 % » | pendant le geste, au plus toutes les 300 ms, puis au relâcher |
| Température de couleur | 2 200 à 6 500 K | 100 K / 500 K | « 2 700 K, blanc chaud » | idem |
| Volume TV | 0 à 60 | 1 / 5 | « Volume 18 sur 60 » | idem |
| Position volet | 0 (fermé) à 100 (ouvert) | 5 / 25 | « Ouvert à 40 % » ; « Fermé » ; « Ouvert » | au relâcher seulement |
| Portions (distributeur) | 1 à 12 | 1 / 3 | « 2 portions » | rien : validé par un bouton |

- **Une seule convention : le pourcentage d'ouverture**, dans l'interface, l'API et les libellés.
  Libellés aux deux bouts de la piste : « Fermé » à gauche, « Ouvert » à droite. La conversion
  Matter ne sort pas du serveur.
- **Le moteur ne court pas après le doigt** : un volet ne reçoit sa cible qu'au relâcher (ou
  0,4 s après la dernière touche clavier). Pendant la course, deux valeurs : la poignée à la cible,
  une marque fine à la position lue, et le texte « Ouvert à 40 % · va à 80 % ». Le bouton
  Arrêter passe en style plein tant que le moteur tourne (rafraîchissement à 1 s, déjà en place).
- **Lampes et volume** suivent le doigt (M3, Apple) mais sans saturer le pont : un envoi au plus
  toutes les 300 ms, le dernier au relâcher. La poignée garde la valeur locale jusqu'à ce que
  l'appareil rapporte une valeur à ± 2 près, ou 3 s ; ensuite elle se range sur la valeur lue.
- Cible tactile : la piste occupe 44 px de haut (poignée visible 20 px, M3), toucher la piste
  place la poignée (2.5.7). `touch-action: pan-y` sur la piste : le défilement vertical de la
  page reste possible sur téléphone.
- Le nom accessible du curseur est son libellé visible (« Luminosité »), l'appareil est donné par
  le groupe (`aria-labelledby` vers le nom de la tuile + le libellé).

## 4. L'état en direct, sans bouger la page

### Ce que disent les sources

- Un tableau de bord n'interroge pas le serveur quand la page est cachée ([MDN, Page Visibility](https://developer.mozilla.org/en-US/docs/Web/API/Page_Visibility_API)).
- Home Assistant distingue `unavailable` (« cannot reach the device ») de `unknown`, et date
  `last_changed`, `last_updated` et `last_reported` ([HA, state objects](https://www.home-assistant.io/docs/configuration/state_object/)).
- Tenir l'utilisateur informé de l'état du système, en un temps raisonnable ([NN/g, visibility of system status](https://www.nngroup.com/articles/visibility-system-status/)) ;
  un indicateur vit sur l'élément concerné, une notification parle du système
  ([NN/g, indicators](https://www.nngroup.com/articles/indicators-validations-notifications/)).
- Des mises à jour fréquentes dans une région live accablent les lecteurs d'écran ; la région doit
  exister avant son contenu ([MDN, live regions](https://developer.mozilla.org/en-US/docs/Web/Accessibility/ARIA/Guides/Live_regions)).
  Un message de statut porte sur le résultat d'une action, une attente, une progression ou une
  erreur ([WCAG 4.1.3](https://www.w3.org/WAI/WCAG22/Understanding/status-messages.html)).
- Des places stables : réorganiser la page casse la mémoire spatiale ([NN/g, spatial memory](https://www.nngroup.com/articles/spatial-memory/)).

### Pour Maison

- **Cadences** (par appareil, pas par page) : 1 s pour un volet en mouvement, 3 s lampes, 5 s
  prises, 10 à 15 s Tuya et volets au repos, 60 s TV et box, 120 s Nabaztag. Tout s'arrête quand
  `document.hidden` et repart, avec une lecture immédiate, au retour sur la page.
- **Fraîcheur** : chaque appareil porte `lastReported`. Au-delà de 3 cadences sans réponse, la
  tuile passe à « Injoignable depuis 3 min » (`<time datetime>`, relatif jusqu'à 59 min, puis
  l'heure : « depuis 14 h 20 »), avec un cercle barré `--status-down` et le texte en
  `--status-down-text`. Le geste reste visible mais `aria-disabled="true"` avec sa raison en
  `aria-describedby`. On n'efface jamais la dernière valeur connue : elle passe en `--ink-muted`
  avec « dernière valeur ».
- **Rien ne bouge** : chiffres en `font-variant-numeric: tabular-nums`, la place du fait de droite
  réservée sur sa plus longue forme (« 100 % », « 2 300 W »), aucune tuile ne change de taille,
  de place ni d'ordre quand un état change. Le squelette de chargement a la hauteur finale de la
  tuile.
- **Annonces** : une seule région `role="status"`, présente dès le chargement (le bandeau
  d'Ariane, au-dessus de la barre du bas). Elle ne parle que du résultat d'un geste de la personne
  qui tient l'écran : échec, fin d'un appairage, fin d'une installation. Jamais pour un
  rafraîchissement, ni pour ce que l'autre personne a fait depuis son téléphone.
- **Ce que fait l'autre** : quand un état change sans geste local, la tuile le dit en ligne 2
  pendant 10 s (« Éteinte à l'instant »), sans animation sous `prefers-reduced-motion`.

## 5. Groupes, ordre et navigation

### Ce que font les références

- **Google Home** : un onglet Favoris, que l'on édite et réordonne à la main, et « All devices »
  rangés par pièce ([Google, l'appli Home](https://support.google.com/googlehome/answer/7071794?hl=en)).
- **Home Assistant** : le tableau par défaut range par zone (pièce), puis par domaine (lumières,
  volets) ([HA, dashboards](https://www.home-assistant.io/dashboards/dashboards/)).
- **NN/g** : les interfaces adaptatives qui déplacent les éléments selon l'usage cassent la
  mémoire spatiale ; dupliquer les fréquents dans une zone à part ne la casse pas
  ([NN/g, spatial memory](https://www.nngroup.com/articles/spatial-memory/)).
- **Barre du bas** : trois à cinq destinations, toujours une active, libellés obligatoires
  ([M3 navigation bar](https://github.com/Glavo/md3-reference-hub/blob/main/docs/components/navigation-bar/guidelines.md)) ;
  pour naviguer, pas pour agir ; pas d'onglet « Plus » ; ne jamais cacher un onglet
  ([HIG Tab bars](https://developer.apple.com/design/human-interface-guidelines/tab-bars)) ;
  au-delà de cinq, les cibles rétrécissent ([NN/g, mobile navigation](https://www.nngroup.com/articles/mobile-navigation-patterns/)).

### Pour Maison

- **Par type d'appareil, pas par marque ni par pièce** (décision du 2 octobre 2026 : un studio,
  une seule pièce ; les pièces reviendront avec un logement qui en a). Les sections nomment ce
  qu'on pilote (« Volets », « Lampes Hue », « Prises », « Coin du chat »), chacune un `h2`
  (`--t-group`, Fraunces 19 px) et ses tuiles en liste. Le protocole n'apparaît que dans le
  détail, sauf là où deux familles cohabitent (Hue et Zigbee). Pas de groupe « Maintenant » :
  une seule pièce, tout tient à l'écran.
- **Ordre fixe**, celui de la configuration, à toutes les largeurs ; groupes placés en colonnes
  sans trous comme les pelotes d'Ariane (22rem minimum, six colonnes au plus, jamais plus que de
  groupes).
- **Barre du bas (< 600 px), trois destinations** : Accueil, Tempo, Télécommande. Les pages
  d'appareil ne sont pas des destinations : on y arrive par une tuile, l'onglet Accueil reste
  actif, et un lien « Retour à l'accueil » en tête de page. Dès 600 px, les trois mêmes dans
  l'en-tête collant, compte et déconnexion à droite.
- Le pavé de la TV n'est pas « la télécommande » : il vit dans la page de la TV (« TV du
  salon »). Le mot « Télécommande » désigne la page de configuration des touches infrarouges,
  pour qu'un même mot ne nomme pas deux écrans.

## 6. Actions longues ou sans retour

### Ce que disent les sources

- Confirmer seulement ce qui est grave et irréversible, sinon la confirmation devient un réflexe ;
  dire la conséquence ; boutons au verbe précis (« Supprimer le fichier » / « Garder ») ; pas de
  choix par défaut ; l'annulation vaut mieux qu'une confirmation ([NN/g, confirmation dialogs](https://www.nngroup.com/articles/confirmation-dialog/)).
- Le bandeau d'annulation : 4 à 10 s, une seule action, un seul à la fois, au-dessus de la barre
  du bas ([Material, snackbar](https://m2.material.io/components/snackbars)) ; GOV.UK : un seul
  bandeau par page, avec parcimonie ([GOV.UK, notification banner](https://design-system.service.gov.uk/components/notification-banner/)).
- Au-delà de 10 s, un indicateur de progression qui dit ce qui reste ; à défaut de pourcentage,
  le détail de ce qui est fait ([NN/g, response times](https://www.nngroup.com/articles/response-times-3-important-limits/)).

### Pour Maison

| Action | Retour possible ? | Traitement |
|---|---|---|
| Retirer un volet ou une lampe (désappairage) | non : il faut réappairer, souvent sur place | dialogue : « Retirer Volet de la chambre ? Il faudra le réappairer avec son code pour le commander à nouveau. » [Retirer le volet] [Garder], aucun bouton focalisé par défaut, le focus sur le titre |
| Supprimer un repas planifié, une touche IR | oui | immédiat, bandeau « Repas de 7 h supprimé · Annuler » 10 s |
| Remettre un compteur à zéro (filtre, pompe) | non, mais sans gravité | dialogue court au verbe précis : [Remettre à zéro] [Garder] |
| Donner des portions, lancer un nettoyage | non, mais voulu | pas de dialogue : la quantité est dans le libellé (« Donner 2 portions »), le résultat sur la tuile (« Servi à 18 h 02 ») |
| Appairer un appareil Matter (~1 min) | — | étapes nommées et temps écoulé : « 1/3 Recherche… », « 2/3 Connexion au réseau… », « 3/3 Configuration… · 42 s » ; on peut quitter la page, la fin s'annonce en `role="status"` |
| Installer un APK sur la box | — | deux temps : envoi en pourcentage (taille connue), puis « Installation… » indéterminé ; résultat avec le nom de l'appli |
| Réveiller la TV (~20 s) | — | § 2 : barre sur le temps écoulé, « jusqu'à 30 s » |

## 7. Le pavé et le volume sur téléphone

### Ce que disent les sources

- Cibles : 44 × 44 pt chez Apple ([HIG Buttons](https://developer.apple.com/design/human-interface-guidelines/buttons)),
  44 × 44 px pour WCAG 2.5.5, utile aux tremblements et à l'usage d'une main
  ([Understanding 2.5.5](https://www.w3.org/WAI/WCAG22/Understanding/target-size-enhanced.html)).
- Répétition à l'appui long : un pas au premier appui, puis après 0,5 s dix pas par seconde
  ([AppKit, NSStepper.autorepeat](https://developer.apple.com/documentation/appkit/nsstepper/autorepeat)) ;
  au clavier, `KeyboardEvent.repeat` signale une touche tenue ([MDN](https://developer.mozilla.org/en-US/docs/Web/API/KeyboardEvent/repeat)) ;
  Shift-clic pour de grands sauts ([HIG Steppers](https://developer.apple.com/design/human-interface-guidelines/steppers)).
- Vibration : chaque motif garde un sens, complète un retour visuel, reste discrète et se coupe
  ([HIG Playing haptics](https://developer.apple.com/design/human-interface-guidelines/playing-haptics)).
  `navigator.vibrate` demande une activation de l'utilisateur ([MDN](https://developer.mozilla.org/en-US/docs/Web/API/Navigator/vibrate))
  et n'existe ni dans Safari iOS ni dans Firefox récent ([caniuse](https://caniuse.com/vibration)).
- Raccourcis d'une seule touche : désactivables, remappables ou actifs seulement quand le
  composant a le focus ([WCAG 2.1.4](https://www.w3.org/WAI/WCAG22/Understanding/character-key-shortcuts.html)).

### Pour Maison

- **Pavé** : quatre flèches de 56 × 56 px autour d'un OK de 64 px, 8 px entre touches ; Retour,
  Accueil et Menu en dessous à 48 px. Le pavé tient dans la largeur d'une main : 216 px au plus.
  Chaque touche a un nom accessible en français (« Haut », « Valider »), jamais le code (`KEY_UP`).
- **Volume** : deux boutons « − » et « + » de 56 px de part et d'autre de la valeur (« 18 »),
  `aria-label` « Baisser le volume » / « Monter le volume ». Le curseur reste dans la page TV.
- **Répétition** : flèches et volume répètent à l'appui long (premier pas, 500 ms, puis 100 ms ;
  le volume plafonne à 5 pas par seconde, ce que la TV suit). Jamais pour Marche, OK, Retour ni
  une touche IR de clim.
- **Vibration** : les motifs de `lib/haptics.ts` gardent leur sens (TAP 8 ms au premier appui,
  CONFIRM 16 ms quand l'appareil confirme, FAILURE à l'échec), rien pendant la répétition, un
  réglage pour couper. Sur iPhone il n'y a pas de vibration web : le retour visuel (état pressé
  de 100 ms au moins) suffit seul.
- **Clavier sur ordinateur**, seulement quand le pavé a le focus (2.1.4) : flèches, Entrée = OK,
  Retour arrière = Retour, `+` / `−` volume, `M` muet. Les touches s'affichent en `<kbd>` sous le
  pavé dès 840 px.

## 8. Tempo : la couleur, toujours avec son nom

### Ce que disent les sources

- RTE fixe la couleur du lendemain à 10 h 30 ; un pré-signal entre 8 h et 10 h 30 ne présage
  pas de l'information définitive ([RTE, calendrier Tempo](https://www.services-rte.com/fr/visualisez-les-donnees-publiees-par-rte/calendrier-des-offres-de-fourniture-de-type-tempo.html)).
  Saison 2026-2027 : 22 jours rouges, 43 blancs, ~300 bleus ; rouges du 1er novembre au 31 mars,
  du lundi au vendredi ([calendrier-tempo.fr](https://www.calendrier-tempo.fr/calendrier), source secondaire).
- La couleur ne porte pas seule l'information ([WCAG 1.4.1](https://www.w3.org/WAI/WCAG22/Understanding/use-of-color.html)) ;
  forme, couleur et texte ensemble ([Carbon](https://carbondesignsystem.com/patterns/status-indicator-pattern/)).
- Calendrier accessible : en-têtes de jour abrégés avec `abbr` au nom complet, flèches pour les
  jours, Page haut/bas pour les mois ([APG, date picker](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/examples/datepicker-dialog/)).

### Pour Maison

- Trois formes en plus des couleurs : **bleu** plein, **blanc** cercle vide 1,5 px
  (`--ink-muted`), **rouge** plein et hachuré à 45°. Le nom en toutes lettres partout où la place
  le permet (tuile, légende, prévisions) ; « B » ne marche pas, bleu et blanc commencent pareil.
- **Jetons** (contrastes calculés le 2 octobre 2026, fond `--ground`) :

| Jeton | Clair | Sombre | Texte dessus |
|---|---|---|---|
| `--tempo-bleu` | `#2B5BA8` (5,82:1) | `#7FA7E8` (7,15:1) | `--ground` clair / `--ground` sombre |
| `--tempo-blanc` | `#FFFFFF` + contour `#525A66` (6,12:1) | `#EAE7E0` (14,15:1) | `--ink` / `#151A21` |
| `--tempo-rouge` | `#B3261E` (5,74:1) | `#F08A7E` (7,19:1) | `--ground` clair / `--ground` sombre |

- **Confirmé ou prévu** : un jour publié par RTE est plein ; une prévision a un contour pointillé
  et sa probabilité (« Rouge probable, 62 % »). Avant 10 h 30, la tuile dit « Demain : annoncé à
  10 h 30 » puis la prévision.
- **Le calendrier** est un `<table>` simple (rien à sélectionner, donc pas de `role="grid"`),
  `<caption>` « Novembre 2026 », en-têtes « lun. » avec `abbr="lundi"`. Chaque cellule se lit
  « mardi 12 novembre, rouge » ; aujourd'hui porte `aria-current="date"` et un contour
  `--accent`. Mois précédent et suivant : deux boutons libellés, plus Page haut/bas quand le
  tableau a le focus.
- La légende est toujours visible au-dessus, avec les compteurs de saison en texte (« Rouges : 4
  sur 22 »).

## Ce que Maison applique

1. Jetons et mise en page d'Ariane sans exception ; ajouts propres à Maison : `--tempo-bleu`,
   `--tempo-blanc`, `--tempo-rouge` (§ 8), `--tile-icon: 48px`, `--remote-key: 56px`,
   `--remote-ok: 64px`.
2. Une tuile = icône-geste 48 px à gauche, nom (lien vers `/appareils/[id]`), état en mots en
   ligne 2, un seul fait à droite, au plus un contrôle sous la ligne. Aucune tuile sans état écrit.
3. État lu : `<button aria-pressed>` nommé par l'appareil. État supposé : deux boutons Allumer et
   Éteindre et le dernier ordre. `role="switch"` seulement dans les listes de réglages.
4. Commande en vol : bascule visuelle immédiate, anneau et « Allumage… » après 1 s, barre au-delà
   de 10 s, retour à l'état lu et « Pas de réponse · Réessayer » après le délai de la famille
   (3 s lampes, 5 s prises, 30 s TV).
5. Curseurs APG : flèche 1 pas, Page 10 pas (volets 5 / 25), Début/Fin ; `aria-valuetext` avec
   l'unité ; piste de 44 px qu'on peut toucher ; lampes et volume envoyés au plus toutes les
   300 ms, volets au relâcher seulement.
6. Volets en pourcentage d'ouverture partout (0 fermé, 100 ouvert) ; la conversion Matter reste
   dans `backend/src/matter.rs`.
7. Rafraîchissement par appareil, coupé quand la page est cachée ; « Injoignable depuis N min »
   après 3 cadences sans réponse, dernière valeur gardée en `--ink-muted`.
8. Aucun déplacement à la mise à jour : `tabular-nums`, place réservée, ordre fixe ;
   un test e2e (comme `e2e/layout.ts` d'Ariane) vérifie qu'aucune tuile ne change de hauteur entre deux rafraîchissements.
9. Une seule région `role="status"`, pour les résultats des gestes de la personne seulement.
10. Groupes par type d’appareil (un studio : pas de pièces pour l’instant), ordre fixe, en colonnes sans trous.
11. Barre du bas : Accueil, Tempo, Télécommande ; les pages d'appareil n'y figurent pas.
12. Dialogue seulement pour l'irréversible (désappairer, remettre à zéro), verbe précis, pas de
    défaut ; tout le reste immédiat avec « Annuler » 10 s.
13. Pavé : touches 56 px, OK 64 px, 8 px d'écart ; répétition 500 ms puis 100 ms sur flèches et
    volume ; raccourcis clavier seulement quand le pavé a le focus ; vibrations optionnelles.
14. Tempo : couleur + forme + nom, jamais une initiale ; prévision en pointillé avec sa
    probabilité ; calendrier en `<table>` avec `aria-current="date"`.
15. Audit axe sur Accueil, Tempo, Télécommande et une page d'appareil, en clair et en sombre, à
    390, 1024 et 2560 px.

## Sources

Ouvertes le 2 octobre 2026.

- Apple : [contrôler les accessoires](https://support.apple.com/guide/iphone/control-accessories-iph0a717a8fd/ios), [accessoire qui ne répond pas](https://support.apple.com/en-us/102056), [HIG Toggles](https://developer.apple.com/design/human-interface-guidelines/toggles), [HIG Sliders](https://developer.apple.com/design/human-interface-guidelines/sliders), [HIG Buttons](https://developer.apple.com/design/human-interface-guidelines/buttons), [HIG Steppers](https://developer.apple.com/design/human-interface-guidelines/steppers), [HIG Tab bars](https://developer.apple.com/design/human-interface-guidelines/tab-bars), [HIG Playing haptics](https://developer.apple.com/design/human-interface-guidelines/playing-haptics), [NSStepper.autorepeat](https://developer.apple.com/documentation/appkit/nsstepper/autorepeat)
- Google : [contrôler ses appareils](https://support.google.com/googlenest/answer/7073578?hl=en), [l'appli Home](https://support.google.com/googlehome/answer/7071794?hl=en)
- Home Assistant : [tile card](https://www.home-assistant.io/dashboards/tile/), [features](https://www.home-assistant.io/dashboards/features/), [dashboards](https://www.home-assistant.io/dashboards/dashboards/), [state objects](https://www.home-assistant.io/docs/configuration/state_object/), [entity](https://developers.home-assistant.io/docs/core/entity/), [cover](https://developers.home-assistant.io/docs/core/entity/cover/), [ha-entity-toggle.ts](https://github.com/home-assistant/frontend/blob/dev/src/components/entity/ha-entity-toggle.ts), [matter/cover.py](https://github.com/home-assistant/core/blob/dev/homeassistant/components/matter/cover.py)
- Matter : [window-covering.xml](https://github.com/project-chip/connectedhomeip/blob/master/src/app/zap-templates/zcl/data-model/chip/window-covering.xml)
- W3C : [APG Switch](https://www.w3.org/WAI/ARIA/apg/patterns/switch/), [APG Button](https://www.w3.org/WAI/ARIA/apg/patterns/button/), [APG Slider](https://www.w3.org/WAI/ARIA/apg/patterns/slider/), [APG color viewer slider](https://www.w3.org/WAI/ARIA/apg/patterns/slider/examples/slider-color-viewer/), [APG date picker](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/examples/datepicker-dialog/), WCAG 2.2 : [1.4.1](https://www.w3.org/WAI/WCAG22/Understanding/use-of-color.html), [1.4.11](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html), [2.1.4](https://www.w3.org/WAI/WCAG22/Understanding/character-key-shortcuts.html), [2.5.5](https://www.w3.org/WAI/WCAG22/Understanding/target-size-enhanced.html), [2.5.7](https://www.w3.org/WAI/WCAG22/Understanding/dragging-movements.html), [2.5.8](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html), [4.1.3](https://www.w3.org/WAI/WCAG22/Understanding/status-messages.html)
- Material : [M3 cards](https://github.com/Glavo/md3-reference-hub/blob/main/docs/components/cards/guidelines.md), [M3 switch](https://github.com/Glavo/md3-reference-hub/blob/main/docs/components/switch/guidelines.md), [M3 sliders](https://github.com/Glavo/md3-reference-hub/blob/main/docs/components/sliders/guidelines.md), [M3 sliders, accessibilité](https://github.com/Glavo/md3-reference-hub/blob/main/docs/components/sliders/accessibility.md), [M3 slider tokens](https://raw.githubusercontent.com/material-components/material-web/main/tokens/versions/v0_192/_md-comp-slider.scss), [M3 navigation bar](https://github.com/Glavo/md3-reference-hub/blob/main/docs/components/navigation-bar/guidelines.md), [snackbar](https://m2.material.io/components/snackbars)
- NN/g : [response times](https://www.nngroup.com/articles/response-times-3-important-limits/), [progress indicators](https://www.nngroup.com/articles/progress-indicators/), [toggle switches](https://www.nngroup.com/articles/toggle-switch-guidelines/), [confirmation dialogs](https://www.nngroup.com/articles/confirmation-dialog/), [spatial memory](https://www.nngroup.com/articles/spatial-memory/), [visibility of system status](https://www.nngroup.com/articles/visibility-system-status/), [indicators, validations, notifications](https://www.nngroup.com/articles/indicators-validations-notifications/), [mobile navigation](https://www.nngroup.com/articles/mobile-navigation-patterns/)
- Design systems : [Carbon, status indicators](https://carbondesignsystem.com/patterns/status-indicator-pattern/), [Carbon, loading](https://carbondesignsystem.com/patterns/loading-pattern/), [Primer, loading](https://primer.style/product/ui-patterns/loading/), [GOV.UK, notification banner](https://design-system.service.gov.uk/components/notification-banner/)
- Web : [MDN, live regions](https://developer.mozilla.org/en-US/docs/Web/Accessibility/ARIA/Guides/Live_regions), [MDN, Page Visibility](https://developer.mozilla.org/en-US/docs/Web/API/Page_Visibility_API), [MDN, KeyboardEvent.repeat](https://developer.mozilla.org/en-US/docs/Web/API/KeyboardEvent/repeat), [MDN, Navigator.vibrate](https://developer.mozilla.org/en-US/docs/Web/API/Navigator/vibrate), [caniuse, vibration](https://caniuse.com/vibration), [React, useOptimistic](https://react.dev/reference/react/useOptimistic)
- Tempo : [RTE, calendrier Tempo](https://www.services-rte.com/fr/visualisez-les-donnees-publiees-par-rte/calendrier-des-offres-de-fourniture-de-type-tempo.html), [calendrier-tempo.fr](https://www.calendrier-tempo.fr/calendrier) (source secondaire)
