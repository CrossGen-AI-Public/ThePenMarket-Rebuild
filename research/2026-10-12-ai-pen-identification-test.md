# Can an AI vision model identify vintage pens from our photos? (test, 2026-10-12)

## Question and why

Action item from the 2026-10-05 meeting with Nathaniel: "test AI image identification using pen photos already on the website". The point is to decide whether a photo-upload helper on the "Sell my pens" page is worth building.

Nathaniel's warnings from that meeting, which this test was set up to check:

- tiny details can make a $50 pen a $500 pen;
- the same pen turned slightly reads as a different model;
- many Montblancs are fakes;
- uncertain or possibly valuable pens must go to him, never be rejected.

## Method

- **Model under test:** Claude (Opus 5.5) looking at each image directly. A production helper might run on a different model, and its results could differ.
- **Sample:** 30 live products with at least one photo, 20 from `vintage-pens` and 10 from `pre-owned-pens`. Brands were spread by `brand_id` only: a per-brand quota, at most 4 per brand, chosen without looking at brand names. Within each brand, products were picked by a fixed hash of the id (`md5(id || 'pen-id-2026')`).
- **Blind step:** the first query returned only product ids and the path of the first photo (`position = 0`, the 960px variant where it exists). A script copied the files to scratch under neutral names (P01 to P30), so neither the file names nor the slugs were seen. For each photo I wrote down brand, model, era, filling system, nib and trim notes, a confidence level and whether I would send it to an expert. That sheet was finished and fingerprinted before any metadata was queried.
- **Scoring:** after the blind sheet was complete I queried the title, brand, era, filling mechanism and the start of `description_text`.
  - **Brand:** right or wrong. P01 is scored against the maker named in its title (Tibaldi), because its brand field is "Other". P16 has no maker in the data and is left out of brand scoring.
  - **Model:** right if I named the model the listing names. Partly if I named the right line but the wrong or missing variant, or hedged between two models with the right one included. Wrong if I named the wrong model or gave no model. P21 is left out of model scoring because the listing itself says the maker's model name is unknown.
  - **Era:** right if my range overlaps the listing's era bucket. The buckets are wide (for example 1900-1919 and 1980-Present), so this score is lenient.
- **Angle test:** I took 3 pens identified correctly from the first photo and looked at one later photo of each. These were not blind, because the truth was already known. I judged what I could have concluded from that photo alone.

## Results

| Code | id | Blind guess (confidence: brand / model) | Truth (title; era; filling) | Brand | Model | Era | Routed to expert? |
|---|---|---|---|---|---|---|---|
| P01 | 126 | Tibaldi (cap imprint); unnamed gold-overlay pen sold with a thermometer; 1930s-40s; filling unclear (high / low) | Tibaldi Penna del Dottore; 1930-1939; eyedropper | right | wrong | right | yes |
| P02 | 77 | Waterman? Gold-filled repoussé overlay hard-rubber eyedropper; 1900s-10s (low / low) | Waterman 0322 (chased taper cap); 1900-1919; eyedropper | right | wrong | right | yes |
| P03 | 187 | Unknown European spiral-fluted metal pen with separate reservoir tube; 1910s-20s (low / low) | Eagle Aluminum Cable Twist (Eagle 1470, glass cartridge); Pre-1900; cartridge | wrong | wrong | wrong | yes |
| P04 | 78 | Sheaffer flat-top lever filler, chased hard rubber; early 1920s (high / medium) | Sheaffer 41 Autograph; 1920-1929; lever | right | wrong | right | no |
| P05 | 152 | Conklin Endura, blue; late 1920s; lever (high / high) | Conklin Endura; 1920-1929; lever | right | right | right | no |
| P06 | 137 | Esterbrook J-series, grey; 1940s-50s; lever (high / medium) | Esterbrook LJ; 1950-1959; lever | right | partly | right | no |
| P07 | 81 | Parker Lucky Curve, black chased hard rubber; button filler; 1910s-early 20s (high / medium) | Parker 25 1/2 (oversized); 1900-1919; eyedropper | right | partly | right | no |
| P08 | 110 | Esterbrook J/SJ, grey, 15xx nib; 1940s-50s (high / high) | Esterbrook J-1555 Gregg; 1950-1959; lever | right | right | right | no |
| P09 | 248 | Cartier Panthère-style ballpoint, blue lacquer; 1980s-90s (medium-high / low) | Cartier Panthère BP; 1980-Present; ballpoint | right | right | right | yes |
| P10 | 217 | OMAS Arte Italiana, Paragon or Milord, blue; modern; piston (high / medium) | Omas Paragon ("Blue Royale"); 1980-Present; piston | right | partly | right | no |
| P11 | 11 | Parker Jotter set of four; 1960s-70s (high / high) | Parker Jotter Set; 1960-1979; ballpoint | right | right | right | no |
| P12 | 105 | Sheaffer Balance Lifetime, black; 1930s-early 40s; lever (high / medium) | Sheaffer Balance Lifetime; 1930-1939; lever | right | right | right | no |
| P13 | 113 | Gold Starry No. 257 (barrel imprint), French safety; 1920s (high / medium) | Gold Starry No. 257 (incomplete: no nib or feed); 1920-1929; safety | right | right | right | yes |
| P14 | 175 | Marlen "Idol" (misread cap band); modern (high / medium) | Marlen (dot) com; 1980-Present; piston | right | wrong | right | no |
| P15 | 82 | Waterman's Ideal No. 12-class straight-cap **eyedropper**; 1900s-10s (high / medium) | Waterman 12 SF (**sleeve filler**) in original box; 1900-1919 | right | partly | right | no |
| P16 | 186 | Unknown maker; "Feng Shui" calligraphy dip-pen set; modern (low / low) | Calligraphy Set; 1980-Present; dip | n/a | right | right | yes |
| P17 | 122 | Sheaffer flat-top lever filler, orange-red; 1920s (high / medium) | Sheaffer 5-30; 1920-1929; lever | right | wrong | right | no |
| P18 | 108 | Eversharp Symphony, blue; late 40s-early 50s; lever (high / medium) | Wahl-Eversharp Symphony; 1950-1959; lever | right | right | right | no |
| P19 | 37 | Pelikan M200-class, blue marbled; modern; piston (high / medium) | Pelikan Souverän M200; 1980-Present; piston | right | right | right | no |
| P20 | 142 | Parker Challenger (guess) with pencil; late 1930s; button filler (high / low) | Parker Deluxe Challenger (factory stub nib); 1930-1939; button | right | partly | right | no |
| P21 | 14 | Ranga, model not determinable; modern (high / low) | Ranga Blue-and-Tan Fantasy (shop's name; maker's model unknown); 1980-Present; eyedropper | right | n/a | right | no |
| P22 | 99 | Lamy Safari ballpoint, charcoal; modern (high / high) | Lamy Safari (2nd gen, W. Germany); 1980-Present; ballpoint | right | right | right | no |
| P23 | 232 | Montblanc 1950s gold-cap, "644 or 744", read cap as Meisterstück (medium / low) | Montblanc 644, cap reads "Masterpiece" (English); 1950-1959; piston | right | partly | right | yes |
| P24 | 243 | Eagle Pencil Co. eyedropper with two repoussé bands; 1900s-10s (high / low) | Eagle Farmers' National Bank (bank imprint on cap); 1900-1919; eyedropper | right | partly | right | no |
| P25 | 79 | **Rexall** drugstore pen (read the clip); 1920s; button filler (medium / low) | **Parker Lucky Curve**, early button filler with an aftermarket Rexall clip; 1900-1919 | wrong | wrong | wrong | no |
| P26 | 203 | Montblanc special edition; guessed Andy Warhol or Frank Lloyd Wright (medium / low) | Montblanc Thomas Mann, Writers Edition 2009; 1980-Present; piston | right | wrong | right | yes |
| P27 | 242 | S.T. Dupont? Ballpoint, red lacquer over guilloché (low / low) | S.T. Dupont Olympio; 1980-Present; ballpoint | right | wrong | right | yes |
| P28 | 98 | Conklin or another 1920s US maker, black flat-top lever filler with orange ring (low / low) | Carter's "FrankenPen" (Carter's cap, oversized Carter's tail, Conklin nib); 1920-1929; lever | wrong | wrong | right | yes |
| P29 | 181 | Cross Townsend, chrome; modern; cartridge/converter (high / medium) | Cross Townsend; 1980-Present; cartridge/converter | right | right | right | no |
| P30 | 83 | Eversharp Skyline, green, striped cap; 1940s; lever (high / high) | Wahl-Eversharp Skyline (uncommon bandless cap); 1940-1949; lever | right | right | right | no |

## Totals

| Measure | Result |
|---|---|
| Brand | 26 of 29 right (90%). P16 has no maker. |
| Model | 12 of 29 right (41%), 7 partly (24%), 10 wrong (34%). P21 excluded. |
| Era (overlap with a wide bucket) | 28 of 30 (93%) |
| Filling system, where I committed to one | 23 of 27. The 4 misses were P01, P03, P07 and P15, the oldest and rarest pens in the sample. |

Accuracy by stated confidence:

| Confidence | Brand | Model |
|---|---|---|
| High | 21 of 21 right | 5 of 5 right |
| Medium | 3 of 4 right (P25 wrong) | 5 of 12 right, 4 partly, 3 wrong |
| Low | 2 of 4 right | 2 of 12 right, 3 partly, 7 wrong |

What the brand calls rested on:

| Evidence | Brand right |
|---|---|
| Readable imprint, logo or box text in the photo | 21 of 22. The miss was P25, where the imprint belonged to a replacement clip. |
| Shape or design only | 5 of 7 |

Routing: I sent 10 of 30 to an expert. Of the 10 model misses, 6 were routed and 4 were not (P04, P14, P17, P25). Of the 3 brand misses, P25 was not routed.

The confidence ratings were useful at the extremes, though the sample is small: "high" was never wrong. "Medium" did not separate good calls from bad ones.

## Angle test

The first photo on each listing is a full view of the pen. Later photos on these listings are mostly nib and detail close-ups, so this tested a different view, not a slightly turned one.

| Pen | Other photo | What I could conclude from that photo alone |
|---|---|---|
| P05 Conklin Endura | Position 2: underside of nib and feed | Nothing. No maker or model could be identified. |
| P05 Conklin Endura | Position 1 (extra): nib face | "Conklin ENDURA" is stamped on the nib, so the model is right from the nib alone. |
| P12 Sheaffer Balance Lifetime | Position 1: nib face ("Sheaffer's Lifetime") | Brand right, Lifetime grade right. Balance or flat-top cannot be told from the nib, so the model drops to partly. |
| P30 Eversharp Skyline | Position 1: nib face ("Eversharp 14K Made in USA") | Brand right. Skyline, Symphony and other Eversharps cannot be told apart, so the model drops to partly. |

No photo produced a different wrong answer. Detail shots lose the model, and a nib imprint is only as good as the nib: on P28 the nib was a Conklin on a Carter's pen. Two blind cases show Nathaniel's "slightly different view" problem directly:

- **P25:** the visible cap imprint was an aftermarket Rexall clip, so I named the wrong maker and the wrong decade for an early Parker Lucky Curve.
- **P23:** only part of the cap band was visible. I filled in "Meisterstück" from habit. The cap actually reads "Masterpiece" in English, which is the uncommon detail the listing highlights.

## Where it failed and why

1. **Model numbers and variants.** Most model misses (P02, P04, P07, P15, P17, P20, P24) are pens where the brand and general type were right but the designation was not visible or not recognised: Sheaffer 41 Autograph, Sheaffer 5-30, Parker 25 1/2, Waterman 0322, Waterman 12 SF, Deluxe Challenger, the bank imprint on the Eagle. These are the details that change value, which is Nathaniel's first warning.
2. **Filling systems that look alike from outside.** The Waterman 12 SF (sleeve filler, original box, 1910-15) went down as a plain eyedropper. The Parker 25 1/2 eyedropper went down as a button filler. Neither was sent to an expert. These were the clearest "$50 vs $500" type misses.
3. **Replacement and mixed parts.** P25 (aftermarket clip) and P28 (a pen assembled from three, with a Conklin nib) misled the identification in the direction of the visible part. A model reading imprints will trust the wrong part with confidence.
4. **Rare and early makers.** The 1890s-1900 Eagle glass-cartridge pen (P03) was not recognised at all. The model also had no name for the Tibaldi Penna del Dottore, though it read "Tibaldi" from the cap.
5. **Luxury special editions.** P26: brand and "special edition" were right, but the edition was wrong; I named two others. The Montblanc cap star and design cannot separate a genuine Writers Edition from a well-made fake. Nothing in a photo of this quality settles authenticity.
6. **Misread or half-read text.** P14 (read "Idol" for "(dot) com") and P23 ("Masterpiece") show the model filling in expected text.
7. **Condition and completeness.** P13 was sold as parts with no nib or feed. I noted the nib was not visible but did not flag that the pen was incomplete. A photo helper cannot judge cracks, sac condition or working mechanisms reliably.

## What this means for a sell-page helper

What it could do reasonably well, on photos like these:

- **Sort a box of pens by likely maker** when an imprint or logo is visible: 21 of 22 here.
- **Suggest the general type and decade**, for example "1920s Sheaffer lever filler" or "1950s Esterbrook J-series", as a starting point that Nathaniel corrects.
- **Ask for the photos that matter:** a sharp shot of the barrel imprint, the nib face, the clip, the cap band and the filling mechanism. Most misses came down to one of these not being visible.
- **Flag pens for Nathaniel:** every Montblanc and luxury brand, every pre-1930 pen, anything with an overlay, a box, a mixed-parts look or an unreadable imprint, and anything below high confidence.

What it must never do:

- **Quote a value or a price range.** The model misses were concentrated on exactly the variants that change value.
- **Reject or discourage a pen.** P25, P15 and P07 were early, collectable pens the model under-read without asking for help, so "not worth sending" is not a judgement it can make.
- **Declare a Montblanc or other luxury pen genuine or fake.**
- **Show the user its guess as a fact.** It should be phrased as "this looks like…" for Nathaniel to confirm, and the submission should reach him whatever the model says.

The safest version routes every submission to Nathaniel. AI output would only pre-fill a draft description and a "please look at this" flag, and never filter anything out.

## Caveats

- These are studio photos taken by an expert, on a plain background, in good light, with imprints often turned to the camera and the box shown when there is one. Customer phone photos (blur, glare, clutter, imprints facing away, several pens in one shot) will do worse, possibly much worse. This test is an upper bound, not an estimate of real use.
- The model in this test was Claude (Opus 5.5) looking at single images directly. A production helper might use a different model, a different version, or a smaller and cheaper one, and its accuracy would need its own test on real submissions.
- 30 pens is a small sample. The confidence breakdowns rest on a handful of pens each.
- The scorer and the model under test were the same session. The rules were written before scoring, but "partly" calls involve judgement.
- The era score is lenient because the site's era buckets are wide.
- The angle test was not blind.
