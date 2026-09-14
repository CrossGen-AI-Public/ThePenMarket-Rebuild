# ThePenMarket.com — Evidence Pass (2026-09-13)

Verbatim only. `pages/`=`~/thepenmarket/backup/content/pages/`; `posts/`=`~/thepenmarket/backup/content/posts/`; `crawl/`=`~/thepenmarket/research/docs/crawl-2026-08-17/`; `tax/`=`~/thepenmarket/scrape/taxonomies/`; `csv`=`~/thepenmarket/backup/backend/products.csv`.

## 1. Headlines and taglines (h1/h2)
- Home (`pages/home.html`): h1 "Home"; h2 "What's New", "Shop Your Favorites"; h3 "Vintage Pens", "Pre-Owned Pens", "Pencils", "Best Bargains", "Trading Post", "For Repairs", "About", "Guarantee". No hero copy.
- About: "About Us" / "Your Hub for Buying, Selling and Trading Pens"; "Our Origins", "From Hobby to Business", "Customer Commitment".
- Repairs: "Pen Repairs" / "Vintage Pen Repairs"; "What Can We Repair", "Safe Shipping Guidelines", "Important Notes about Restoration".
- Sell: "How Do I Sell My Pens?" / "Sell My Pens!". Contact: "Reach Out to Our Team". Guarantee: "Our Guarantee".
- Meta description: "Discover unique vintage pens, preowned pens and luxury pens at thepenmarket.com. Sell your pens, too. Learn more."
- Instagram bio (https://www.instagram.com/thepenmarketdotcom/): "Since 2007, ThePenMarket.com has been the top treasure trove for bargains on vintage pens and preowned luxury pens."

## 2. Product description voice (`csv`)
- SKU 6961: "Here's a tasty treat for your weekend: a Waterman 56."
- SKU 6957: "Here is something we suspect you've never seen before: a Conklin 75 with solid 14k gold accents."
- SKU 6607 ($16,499.99): "Be careful, as this is a vintage mercury thermometer. Being Italian, the temperature reads in centigrade."
- SKU 6967: "Bordering on grail territory is the Montblanc 644."
- SKU 6687 ($69.99): "Everyone needs a great vintage pen daily driver. We think that this Parker 51 Special just might be that pen for you. It certainly worked well for Helen Bronnenberg."
- SKU 6843: "Sometimes basic is best."
- SKU 6960: "Marked "F" for fine, we think it is a really generous fine. We would call it a medium."
- Waterman Charleston (`crawl/product_pre-owned-pens-waterman-charleston.html`): "Neither rare nor exotic, our vintage-modern heart finds this Waterman Charleston to be an absolute joy to own and write with. 13.4cm capped."
Every description ends with capped length; flaws stated plainly ("No cracks, brassing or deep scratches.").

## 3. Blog voice (`posts/`)
- `welcome-to-the-pen-market-coms-blog` (2013-09): "It is a space dedicated to pen lovers old and new." Signed "Nathaniel Cerf / President, Repairman".
- `welcome-to-the-new-improving-thepenmarket-com` (2025): "We love the swashbuckling fountain pen crossing the T in this new logo that also features a drop of ink for the period of .com."
- `how-do-i-start-collecting-pens-know-thy-obsession` (2017-09-28): "There are soooooo many great pens out there in need of a good home. Where do you begin?"
- `dont-get-fooled-by-fake-mont-blancs-vermeil-solitaire` (2026-05-15): "A friend was perusing the world's most famous electronic auction site (rhymes with uPay)"
- `how-do-i-restore-a-parker-vacumatic` (2024-10-19): "ANNNND, for the love of all that you hold dear, keep celluloid pens away from open flames."
- `pen-scam-alert` (2026-03-01): "For 20 years, our Trading Post as been a safe place for pen lovers to buy, sell and trade pens." (typo in source)

Counts (`~/thepenmarket/backup/content/posts_index.csv`: 261 posts, 2013-09-14 to 2026-07-30; `tax/categories.json`): fake-pen titles 10; Ink Reviews 29 (UV/pH to `summers-sunburned-inks` 2026-07-18); Famous People & Pens 31, presidential-titled 6; "How Do I Start Collecting Pens?" category 17, title-prefixed 5; Notes from the Work Bench 19, "How Do I Restore/Re-sac/Replace/Polish" titles 6; pen-show titles 18 (last 2020-03-20); Pen World titles 7.

## 4. CTAs and nav (`~/thepenmarket/scrape/pages/home.md`)
Header: Home | On Sale! | Vintage Pens | Pre-Owned Pens | Pencils | Cameras | How Do I Sell My Pens? | Pen Repairs | Inkwell & Blotters | Blog | Trading Post; "(847) 708-5062"; "Contact Us". Footer: "Site Links" (+About Us); "Quick Links": Shop, Cart, Checkout, Guarantee; "Join Our Mailing List" / "Submit".
Buttons: "Ask Us Anything"; "Get Free Repair Estimate"; "Read More about {product}"; "Click here to list a single item on the Trading Post"; "Start an Unlimited Post Membership Click Here". Home tile: "It only costs $5 per listing, and you keep your pen until someone pays you."

## 5. Legal, guarantee, disclaimers
- Footer: "P.O. Box 1086 / Norwich, CT 06360-1086" · "(847) 708-5062" · "info@thepenmarket.com" · "© 2026, ThePenMarket.com. All Rights Reserved." · "GoDaddy Web Design" badge.
- Guarantee (`pages/guarantee.html`): "Customer satisfaction is our top priority at ThePenMarket.com, and we will do whatever it takes to make you happy with your purchase. If you are not satisfied with your purchase for any reason, return it in the same condition it arrived via insured U.S. Mail within 14 days of receiving it, and you will be refunded the full price of the item, not including the shipping. If you discover any problems we might have missed within the first 30 days of receiving your pen, let us know and we'll do what we can to fix it."
- Trading Post (`pages/trading-post.html`): "DISCLAIMER: While we at ThePenMarket.com think most pen people are pretty honest folks, we want to make it clear that we cannot vouch for each item, buyer or seller on this page. This is simply a giant "classified ads" catering to fountain pens, pencils and writing ephemera. The merchandise is not guaranteed by ThePenMarket.com, nor does the transaction pass through our hands. We do our best to vet out the "bad" dealers, and ask that you report any problems immediately to tradingpost@ThePenMarket.com. We will try to help to the best of our ability, but as we do not handle the money or pens during the transactions generated by our Trading Post, we make no guarantees."
- Repairs (`pages/pen-repairs.html`): "Button Fillers / Lever Fillers / Parker Vacumatics / Sheaffer Snorkels / Sheaffer Touchdowns / Crescent Fillers / Aerometrics". "*Please note that we are no longer restoring European, Nozac and Sheaffer piston fillers." "By submitting your pen for restoration, you acknowledge these risks and waive any claims in the event of damage."
- Sell (`pages/sell-my-pens.html`): "The only things we are not looking for are cheap advertising pens, Cross Century pens and falling apart third-tier vintage pens. (We're looking at you Wearever.)"

## 6. Numbers
- "In 2007 he expanded the venture to the internet" (About). "the 9th anniversary of ThePenMarket.com's launch" on 2016-08-05 (`posts/my-first-published-book-little-victories.html`). "more than 20 years of experience restoring" (Repairs). "For 20 years, our Trading Post" (2026).
- `tax/product_cat.json`: Vintage Pens 110, Pre-Owned Pens 110, Pencils 33, Inkwells & Blotters 5, Camera 3, Memberships 1. `csv`: 248 rows, 37 on sale, $59.99 to $16,499.99. Brands 67.
- Trading Post: 107 listings (`~/thepenmarket/scrape/trading-post/`); sitemap 79. "$5 per listing". "Unlimited Posts" $125: "Upload as many writing instruments and related ephemera as you want for one calendar year".
- Guarantee 14 days / 30 days. "Discounts are available for shipments of 5 or more pens." No repair price list. Instagram 372 followers (today). Facebook "3.3K" (snippet, unverified).

## 7. Founder facts
- About: "fountain pen junkie since the age of 9, when he first found his late grandfather's Sheaffer Lifetime Balance"; "while teaching a fencing class in Sioux Falls, South Dakota, Cerf met a fountain pen repairman"; "After moving to the Chicagoland area"; "with the help of Chris & Ruth Gagliano at Computer Friendly Associates".
- Novel "Little Victories", Last Chance Press, 2016 (post above); Amazon ISBN 9780692649510 (rating unverified).
- `posts/catching-up-part-i-our-first-pen-world-write-up` (2017-05-20): "thank 'Pen World Magazine' for writing a story about my novel 'Little Victories' in the December 2016 issue"; "nominated for (and losing) a Pulitzer Prize"; "I am now working full-time for myself at ThePenMarket.com". Snippet only: "master's degree in journalism from The University of Montana" — unverified.
- `posts/grab-some-popcorn-its-podcast-time` (2023-05-24): "'Drawing with Fountain Pens'" by Jonathan Weinberg, "founder of the Charter Oak Pen Club and curator at The Maurice Sendak Foundation". Episode URL not found.
- The Day, 2022-04-06, https://www.theday.com/local-news/20220406/norwich-resident-specializes-in-modern-vintage-pens/: "I think people really like the experience of unplugging (from the computer) and actually having a tactile moment"; "an explosion of interest in ink"; "Parker and Sheaffer pens from probably the 1920s into the 1940s and 1950s, which was really sort of the golden age of fountain pens."

## 8. Reviews and proof
- The Day (above), Dr. Tobias Goodman, "president of the North Stonington Historical Society": "He's very sociable and interesting, but he's also sincere, ethical and knowledgeable and he does a very good job with these fountain and dip pens."
- FPN https://www.fountainpennetwork.com/forum/topic/265070-any-experience-with-the-pen-marketcom/ (5 replies, no negatives): cellmatrix 2014-04-20: "I bought an esterbrook from them once. The proprietor seemed like a decent fellow and I was very pleased with the pen and service." orfew: "Great communication and fast shipping."
- Facebook reviews tab login-walled. None found: Trustpilot, BBB, Yelp, Google Business, Reddit. Site: "There are no reviews yet."; no testimonials.

## 9. Brand (`crawl/home.html`)
Logo: red wordmark PNG 1645x381 (`uploads/2021/08/Picture1.png`; black-bg `Picture1-1.png`). CSS: accent `#d50003`; text `#131313`; headings `#000000`; bg `#ffffff`; nav/links `#428bca` (Beaver Builder default); borders `#e6e6e6`. Fonts: Mulish 300/400/700, DM Sans 700. Imagery: Adobe Stock tiles, own "landscape" product photos.

## 10. Technical
WordPress 7.0.4, WooCommerce 11.0.1, WP Rocket 3.23.1.1, bb-theme + Beaver Builder/PowerPack/UABB, Search & Filter Pro, FiboSearch, WC Bookings/Memberships/Subscriptions/Table Rate Shipping. home.html 1,177,703 bytes; 27 stylesheets, 34 scripts; nav in DOM 3x; home h1 "Home"; reviews enabled on 126/248 products, none exist.

## 11. Unverified / conflicting
- "one of the fastest-growing pen retailers in the country" (About) — no source.
- "at pen shows all around the United States" — last show post 2020-03-20.
- (847) Chicago area code vs Norwich CT PO box.
- `/refund_returns/` stock WooCommerce ("30 days", "Sale items cannot be refunded", "{physical address}", CDs/VHS) vs 14-day Guarantee.
- "20 years" Trading Post (2026) vs 2007 launch = 19; "more than 20 years" restoring undated.
- Trading Post 107 (JSON) vs 79 (sitemap) vs 76 (DISCOVERY); products 110/110 (tax) vs 107/98 (DISCOVERY).
- Facebook 3.3K, Montana degree, Amazon rating: snippets only. DISCOVERY tagline "Buy, sell and trade pens the way you want." not re-verified.
