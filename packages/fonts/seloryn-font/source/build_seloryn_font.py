import re, math, copy
from pathlib import Path
from shapely.geometry import LineString, Polygon, MultiPolygon, GeometryCollection
from shapely.ops import unary_union
from shapely.geometry.polygon import orient
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont
from fontTools.feaLib.builder import addOpenTypeFeaturesFromString

OUT=Path(__file__).resolve().parent.parent / 'fonts'
OUT.mkdir(parents=True,exist_ok=True)
# Hand-designed monoline skeletons. SVG-style coordinate system with y positive upward.
# Individual subpaths remain independent unless their strokes intentionally intersect.
G={}
W={}
def add(c, *paths, width=None):
    G[c]=list(paths);W[c]=width if width else (640 if c.isupper() else 560)
# Original logo translated from assets/brand/seloryn-symbol.svg into a font cap-height geometry.
add('S','M526 635 L281 635 C170 635 118 557 118 447 C118 337 175 285 281 285 L416 285',
       'M142 65 L387 65 C502 65 555 142 555 259 C555 369 500 427 391 427 L254 427',width=650)
# Geometrically soft uppercase, softened junctions and deliberately autonomous cross-bars.
add('A','M90 40 L274 634 Q307 696 344 634 L536 40', 'M170 260 L462 260',width=634)
add('B','M112 50 L112 640','M150 635 L335 635 C475 635 510 578 510 495 C510 412 451 356 337 356 L146 356',
        'M146 349 L350 349 C488 349 535 293 535 203 C535 105 473 56 343 56 L152 56',width=640)
add('C','M534 570 C475 629 416 661 314 661 C168 661 100 533 100 342 C100 159 174 30 323 30 C421 30 477 67 535 122',width=635)
add('D','M112 50 L112 640','M149 635 L315 635 C492 635 545 520 545 344 C545 168 489 56 312 56 L150 56',width=648)
add('E','M535 635 L253 635 C152 635 116 587 116 492 L116 179 C116 95 159 54 253 54 L535 54','M148 347 L464 347',width=626)
add('F','M115 50 L115 496 C115 592 156 635 258 635 L533 635','M145 346 L461 346',width=614)
add('G','M534 566 C468 640 409 662 310 662 C155 662 95 530 95 347 C95 153 167 28 320 28 C463 28 546 119 546 265 L546 331 L359 331',width=644)
add('H','M112 635 L112 50','M532 635 L532 50','M147 340 L499 340',width=646)
add('I','M128 635 L499 635','M314 606 L314 79','M128 50 L499 50',width=622)
add('J','M504 635 L504 199 C504 77 424 28 307 28 C211 28 156 77 112 144',width=616)
add('K','M112 635 L112 50','M533 635 L156 342','M278 350 L543 50',width=644)
add('L','M112 635 L112 173 Q112 54 231 54 L531 54',width=619)
add('M','M88 50 L88 605 Q88 681 152 620 L316 374 L479 620 Q541 681 541 605 L541 50',width=660)
add('N','M109 50 L109 631','M145 628 L531 55','M567 50 L567 635',width=674)
add('O','M320 663 C464 663 547 545 547 346 C547 143 464 27 320 27 C176 27 93 143 93 346 C93 545 176 663 320 663 Z',width=641)
add('P','M112 50 L112 635','M150 635 L325 635 C470 635 529 577 529 473 C529 361 472 307 326 307 L150 307',width=636)
add('Q','M319 662 C462 662 543 544 543 345 C543 143 462 28 319 28 C173 28 92 143 92 345 C92 544 173 662 319 662 Z','M382 174 L547 -6',width=650)
add('R','M112 50 L112 635','M150 635 L318 635 C466 635 525 582 525 476 C525 375 466 322 318 322 L151 322','M326 316 L540 50',width=641)
add('T','M76 635 L553 635','M315 598 L315 50',width=630)
add('U','M109 635 L109 240 C109 91 187 26 316 26 C449 26 527 92 527 240 L527 635',width=637)
add('V','M82 635 L268 60 Q313 -8 357 60 L539 635',width=624)
add('W','M67 635 L176 68 Q191 -3 233 69 L321 389 L410 69 Q452 -3 468 68 L568 635',width=655)
add('X','M107 635 L529 50','M519 635 L99 50',width=636)
add('Y','M92 635 C140 501 231 402 309 320','M532 635 C479 489 405 395 337 320','M323 291 L323 50',width=636)
add('Z','M105 635 L526 635 Q556 635 527 597 L113 92 Q89 50 137 50 L541 50',width=639)
# Lowercase x-height ~465, ascenders 690, descenders -165.
add('a','M472 236 C472 367 414 465 296 465 C179 465 108 365 108 238 C108 106 179 22 296 22 C414 22 472 112 472 236 Z','M474 438 L474 45',width=589)
add('b','M110 683 L110 50','M122 239 C122 365 184 466 300 466 C418 466 493 375 493 240 C493 107 417 23 300 23 C184 23 122 116 122 239 Z',width=577)
add('c','M462 391 C424 445 366 466 298 466 C163 466 103 362 103 244 C103 117 167 23 299 23 C370 23 428 47 466 99',width=548)
add('d','M477 684 L477 48','M466 236 C466 365 404 466 288 466 C171 466 96 375 96 240 C96 107 172 23 288 23 C404 23 466 116 466 236 Z',width=579)
add('e','M457 128 C409 58 364 24 286 24 C149 24 94 125 94 247 C94 375 167 463 284 463 C398 463 477 389 477 294 L476 268','M120 268 L472 268',width=568)
add('f','M209 43 L209 518 C209 647 252 685 382 685','M92 442 L387 442',width=459)
add('g','M470 252 C470 370 408 465 294 465 C177 465 103 369 103 246 C103 123 174 40 291 40 C408 40 470 136 470 252 Z','M471 438 L471 -30 C471 -149 395 -187 283 -187 C218 -187 171 -173 129 -139',width=589)
add('h','M107 682 L107 45','M130 265 C132 392 208 464 302 464 C408 464 470 393 470 274 L470 45',width=578)
add('i','M197 428 L197 46','M196 659 L196 654',width=362)
add('j','M284 428 L284 -67 C284 -153 244 -184 165 -184','M282 659 L282 654',width=379)
add('k','M106 682 L106 45','M440 458 L135 231','M265 269 L470 44',width=563)
add('l','M190 682 L190 140 C190 70 214 41 285 41',width=355)
add('m','M87 45 L87 443','M111 270 C111 400 169 462 249 462 C332 462 366 405 366 285 L366 45','M365 268 C365 396 422 462 500 462 C581 462 621 400 621 281 L621 45',width=724)
add('n','M108 45 L108 442','M130 269 C130 389 209 464 301 464 C409 464 474 392 474 270 L474 45',width=578)
add('o','M290 465 C416 465 485 370 485 246 C485 115 416 23 290 23 C165 23 96 115 96 246 C96 370 165 465 290 465 Z',width=580)
add('p','M108 434 L108 -179','M122 239 C122 364 184 465 300 465 C417 465 493 375 493 240 C493 107 417 23 300 23 C184 23 122 115 122 239 Z',width=580)
add('q','M468 430 L468 -179','M456 238 C456 364 394 465 278 465 C161 465 86 375 86 240 C86 107 161 23 278 23 C394 23 456 115 456 238 Z',width=577)
add('r','M119 45 L119 438','M141 260 C143 377 211 461 317 461 C354 461 397 446 428 425',width=493)
# Small two-strand 's' analog to official symbol.
add('s','M445 446 L267 446 C179 446 127 403 127 332 C127 270 174 240 269 240 L382 240',
        'M125 50 L303 50 C405 50 455 91 455 166 C455 227 408 260 314 260 L199 260',width=573)
add('t','M222 635 L222 156 C222 73 265 38 347 38 C382 38 416 47 445 65','M100 443 L421 443',width=504)
add('u','M104 441 L104 238 C104 93 167 25 286 25 C404 25 473 93 473 239 L473 442','M473 186 L473 44',width=577)
add('v','M83 443 L239 61 Q284 -8 327 61 L480 443',width=559)
add('w','M66 445 L171 59 Q190 -6 227 62 L298 281 L370 62 Q407 -6 425 59 L533 445',width=610)
add('x','M108 446 L463 43','M457 446 L103 43',width=566)
add('y','M92 445 L269 66','M475 445 L295 -13 C239 -149 205 -179 114 -179',width=561)
add('z','M103 443 L458 443 Q482 443 453 407 L121 76 Q94 43 132 43 L471 43',width=564)
# Numerals with generous open counters and rounded terminal vocabulary.
add('0','M310 664 C448 664 519 533 519 342 C519 149 448 25 310 25 C174 25 102 149 102 342 C102 533 174 664 310 664 Z',width=619)
add('1','M144 519 L318 654 L318 47',width=460)
add('2','M116 509 C143 615 229 657 329 657 C445 657 515 592 515 487 C515 390 449 328 351 252 L133 89 Q100 44 158 44 L530 44',width=619)
add('3','M116 563 C169 628 227 657 323 657 C438 657 499 599 499 513 C499 426 432 359 323 359 L230 359','M320 351 C450 351 519 297 519 192 C519 90 444 25 321 25 C229 25 170 58 113 123',width=619)
add('4','M449 45 L449 650','M468 235 L96 235 Q69 235 91 272 L390 648',width=605)
add('5','M502 637 L160 637 L149 374 C200 410 250 428 332 428 C449 428 517 346 517 230 C517 105 444 26 320 26 C221 26 169 61 111 126',width=614)
add('6','M484 592 C439 639 379 661 309 661 C174 661 97 531 97 333 C97 139 171 23 312 23 C445 23 522 110 522 223 C522 343 446 411 326 411 C210 411 124 334 105 238',width=624)
add('7','M91 637 L520 637 L241 45',width=600)
add('8','M307 658 C417 658 481 603 481 515 C481 431 425 370 308 370 C191 370 133 431 133 515 C133 603 197 658 307 658 Z','M307 371 C442 371 516 310 516 199 C516 87 443 23 307 23 C170 23 97 87 97 199 C97 310 171 371 307 371 Z',width=613)
add('9','M140 95 C185 46 245 26 315 26 C453 26 524 157 524 348 C524 543 450 662 309 662 C177 662 100 577 100 464 C100 346 176 273 297 273 C408 273 491 354 517 438',width=622)
# Punctuation designed from spheres, not sharp geometric default shapes.
add('.','M129 48 L129 46',width=278)
add(',','M153 56 Q163 -1 108 -90',width=291)
add(':','M130 397 L130 395','M130 48 L130 46',width=286)
add(';','M138 398 L138 396','M152 55 Q158 -2 109 -88',width=299)
add('!','M148 635 L148 184','M148 48 L148 46',width=301)
add('?','M101 526 C115 615 188 665 289 665 C404 665 477 603 477 505 C477 422 422 372 342 317 C299 289 277 257 277 207','M277 50 L277 48',width=559)
add('-','M99 254 L352 254',width=454)
add('_','M72 -107 L479 -107',width=553)
add('+','M85 286 L492 286','M288 80 L288 493',width=576)
add('=','M95 354 L483 354','M95 201 L483 201',width=581)
add('/','M93 -79 L480 692',width=554)
add('\\','M94 692 L480 -79',width=555)
add('|','M125 690 L125 -165',width=248)
add('(','M338 698 C201 566 157 430 157 265 C157 96 205 -44 337 -178',width=421)
add(')','M86 698 C224 566 268 430 268 265 C268 96 220 -44 87 -178',width=421)
add('[','M337 688 L150 688 L150 -165 L337 -165',width=425)
add(']','M83 688 L270 688 L270 -165 L83 -165',width=425)
add('{','M365 692 C238 692 203 628 203 513 L203 403 C203 307 158 275 90 271 C158 267 203 232 203 136 L203 23 C203 -94 238 -164 365 -164',width=455)
add('}','M92 692 C218 692 254 628 254 513 L254 403 C254 307 299 275 367 271 C299 267 254 232 254 136 L254 23 C254 -94 218 -164 92 -164',width=455)
add('"','M130 648 L130 473','M320 648 L320 473',width=445)
add("'",'M145 648 L145 473',width=273)
add('`','M111 679 L201 547',width=286)
add('~','M95 268 C162 334 210 340 292 278 C376 214 419 215 496 284',width=591)
add('*','M278 616 L278 327','M152 547 L404 392','M152 394 L405 549',width=555)
add('#','M198 620 L141 55','M431 620 L372 55','M93 426 L487 426','M70 249 L464 249',width=567)
add('%','M114 45 L520 648','M210 649 C268 649 295 601 295 545 C295 482 263 445 210 445 C155 445 122 482 122 545 C122 601 153 649 210 649 Z','M423 245 C479 245 513 205 513 145 C513 84 479 43 423 43 C368 43 335 84 335 145 C335 205 367 245 423 245 Z',width=622)
add('&','M531 50 L213 376 C117 467 144 636 281 651 C386 660 452 602 451 512 C450 409 349 356 265 321 C161 274 116 230 116 153 C116 75 172 26 271 26 C383 26 463 109 532 195',width=660)
add('@','M505 356 C505 438 454 478 380 478 C304 478 257 421 257 335 C257 250 302 202 380 202 C456 202 502 259 502 336 L502 469','M501 284 C501 199 539 185 577 207 C622 240 629 312 629 368 C629 530 506 659 329 659 C146 659 76 530 76 343 C76 140 167 26 339 26 C442 26 520 60 586 116',width=714)
add('$','M484 570 C429 636 373 659 294 659 C177 659 119 588 119 494 C119 402 179 364 301 346 C431 327 515 278 515 178 C515 89 444 27 318 27 C218 27 150 65 105 118','M311 730 L311 -38',width=621)
add('€','M515 560 C470 629 401 659 317 659 C180 659 113 523 113 347 C113 154 184 27 327 27 C418 27 479 63 530 130','M72 420 L367 420','M72 274 L352 274',width=641)
add('£','M474 573 C437 632 375 659 305 659 C201 659 162 588 162 496 C162 407 197 320 197 218 C197 132 174 96 119 52 L514 52','M92 322 L396 322',width=624)
add('°','M275 656 C338 656 379 611 379 552 C379 493 338 453 275 453 C211 453 171 493 171 552 C171 611 211 656 275 656 Z',width=545)
add('^','M105 450 L288 649 L466 450',width=567)
add('•','M232 303 L232 301',width=464)
add('–','M100 255 L550 255',width=652)
add('—','M100 255 L799 255',width=900)
add('…','M120 48 L120 46','M365 48 L365 46','M608 48 L608 46',width=740)
add('¡','M148 457 L148 455','M148 320 L148 -136',width=300)
add('¿','M280 459 L280 457','M455 9 C441 -80 367 -130 266 -130 C151 -130 78 -67 78 31 C78 113 133 163 212 220 C256 248 278 280 278 330',width=558)
# Additional English / Brazilian Portuguese punctuation and ordinals.
add('<','M448 517 L123 291 L448 66',width=565)
add('>','M120 517 L447 291 L120 66',width=565)
add('‘','M213 678 C156 632 139 579 172 522',width=330)
add('’','M122 522 C179 568 197 622 163 678',width=330)
add('“','M180 678 C128 633 118 579 151 522','M355 678 C302 633 292 579 325 522',width=502)
add('”','M123 522 C176 568 187 622 155 678','M297 522 C351 568 361 622 328 678',width=502)
add('º','M232 674 C310 674 357 621 357 555 C357 485 310 440 232 440 C157 440 107 485 107 555 C107 621 157 674 232 674 Z',width=467)
add('ª','M363 550 C363 620 322 669 250 669 C179 669 130 620 130 555 C130 485 179 441 250 441 C320 441 363 488 363 550 Z','M367 645 L367 450',width=468)
add('·','M150 286 L150 284',width=302)
add('−','M112 292 L512 292',width=624)
# Seloryn Font / v2: spine-and-flow optical grammar
# Each glyph comprises independent C/return paths instead of default geometric loops.
# Strokes are rounded and overlap only at deliberate structural junctions.
# Tracks have three recurring end forms: horizontal exit, curling return, and separated upright.
# Coordinate system is baseline-up, 1000 units per em, cap about 635, x-height about 465.

# Major letterforms: convert default single rings and mechanical triangles into broken flowing rails.
add('O',
  'M292 663 L348 663 C473 663 544 546 544 345 C544 152 469 27 343 27 L288 27 C163 27 97 151 97 345 C97 540 169 663 292 663 Z',width=645)
add('Q',
  'M292 663 L348 663 C473 663 544 546 544 345 C544 152 469 27 343 27 L288 27 C163 27 97 151 97 345 C97 540 169 663 292 663 Z',
  'M391 170 C440 109 496 41 547 -16',width=663)
add('C',
  'M523 571 C454 628 398 658 310 658 C166 658 99 529 99 344 C99 157 175 30 313 30 C399 30 464 59 534 123',width=642)
add('G',
  'M524 569 C459 634 404 661 310 661 C163 661 96 532 96 347 C96 160 171 28 316 28 C461 28 540 109 540 239 L540 324',
  'M518 327 L360 327',width=656)
add('U','M111 637 L111 247 C111 91 192 27 321 27 C452 27 532 91 532 247 L532 637',width=648)
add('D',
  'M113 52 L113 635',
  'M208 635 L310 635 C469 635 548 521 548 347 C548 166 469 56 309 56 L211 56',width=654)
add('B',
  'M114 50 L114 635',
  'M212 635 L339 635 C466 635 516 574 516 491 C516 408 454 356 341 356 L222 356',
  'M216 339 L341 339 C482 339 535 277 535 195 C535 102 472 55 339 55 L212 55',width=645)
add('P',
  'M112 50 L112 635',
  'M207 635 L330 635 C474 635 531 575 531 470 C531 360 466 301 325 301 L208 301',width=635)
add('R',
  'M112 50 L112 635',
  'M207 635 L320 635 C465 635 529 580 529 479 C529 379 464 321 329 321 L211 321',
  'M338 301 C389 232 473 117 551 50',width=662)
add('E',
  'M528 634 L275 634 C169 634 115 581 115 475 L115 435',
  'M115 270 L115 204 C115 105 166 53 271 53 L528 53',
  'M178 340 L467 340',width=626)
add('F',
  'M115 50 L115 476 C115 584 168 635 274 635 L526 635',
  'M177 339 L461 339',width=618)
add('A',
  'M93 50 L267 578 C287 645 318 665 346 580 L538 50',
  'M177 260 L464 260',width=638)
add('H',
  'M113 636 L113 50','M535 636 L535 50',
  'M175 338 C283 345 390 342 477 338',width=650)
add('I',
  'M132 635 L500 635','M316 581 L316 105','M132 50 L500 50',width=632)
add('K',
  'M111 635 L111 50',
  'M531 635 C434 527 286 397 162 340',
  'M320 318 C398 235 483 137 550 50',width=657)
add('M',
  'M93 50 L93 558 C93 636 127 658 178 585 L319 382',
  'M340 382 L465 581 C514 657 558 639 558 559 L558 50',width=668)
add('N',
  'M111 50 L111 634',
  'M158 590 C262 453 412 214 527 93',
  'M573 635 L573 50',width=680)
add('V',
  'M88 635 L261 111 C290 22 326 4 360 109 L544 635',width=635)
add('W',
  'M66 635 L169 112 C184 32 206 20 231 100 L315 363',
  'M329 363 L412 101 C437 20 459 32 475 113 L580 635',width=662)
add('X',
  'M108 635 C199 521 435 146 543 50',
  'M540 635 C443 515 209 148 104 50',width=660)
add('Y',
  'M98 634 C140 522 227 393 313 312',
  'M545 634 C500 512 418 387 340 312',
  'M325 279 L325 50',width=654)
add('Z',
  'M111 634 L519 634 C544 634 549 620 533 601 C438 489 240 207 126 102 C104 75 107 51 151 51 L544 51',width=650)
add('T',
  'M88 635 L546 635',
  'M317 577 L317 50',width=640)
add('L',
  'M115 635 L115 189 C115 90 167 54 257 54 L530 54',width=624)
# Lowercase bowls: a rounded outer current and autonomous return strand define each counter.
add('o',
  'M286 465 L331 465 C433 465 491 376 491 245 C491 111 430 24 326 24 L274 24 C160 24 99 115 99 245 C99 376 164 465 286 465 Z',width=582)
add('a',
  'M412 441 C371 458 332 465 289 465 C166 465 103 364 103 243 C103 116 171 31 293 31 C337 31 377 43 414 69',
  'M483 440 L483 45',width=588)
add('b',
  'M111 682 L111 45',
  'M184 447 C205 456 249 468 296 468 C416 468 494 371 494 245 C494 109 418 23 294 23 C251 23 209 34 184 48',width=585)
add('d',
  'M480 682 L480 45',
  'M408 447 C387 456 345 468 293 468 C171 468 94 371 94 245 C94 109 170 23 293 23 C337 23 382 34 408 48',width=586)
add('p',
  'M111 442 L111 -179',
  'M184 447 C206 457 250 467 294 467 C416 467 493 372 493 245 C493 109 417 23 293 23 C251 23 210 34 184 49',width=585)
add('q',
  'M480 442 L480 -179',
  'M408 447 C387 457 346 467 293 467 C171 467 94 372 94 245 C94 109 170 23 294 23 C339 23 382 34 408 49',width=586)
add('g',
  'M414 440 C371 458 331 465 290 465 C167 465 102 363 102 244 C102 120 168 38 289 38 C333 38 375 50 413 76',
  'M483 441 L483 -25 C483 -138 408 -182 290 -182 C221 -182 167 -165 124 -134',width=592)
add('e',
  'M449 403 C406 446 354 465 286 465 C165 465 103 366 103 242 L468 242',
  'M137 111 C170 62 218 25 297 25 C372 25 426 59 464 111',width=575)
add('c',
  'M467 397 C427 444 367 465 297 465 C167 465 102 363 102 244 C102 114 169 25 300 25 C365 25 423 47 469 95',width=556)
add('n',
  'M111 45 L111 442',
  'M167 427 C213 458 259 465 307 465 C419 465 478 384 478 265 L478 45',width=583)
add('h',
  'M110 681 L110 45',
  'M168 428 C211 458 259 465 307 465 C420 465 478 383 478 265 L478 45',width=584)
add('m',
  'M95 45 L95 444',
  'M143 427 C186 456 230 464 272 464 C355 464 378 386 378 285 L378 45',
  'M419 427 C460 456 508 464 545 464 C627 464 656 386 656 285 L656 45',width=728)
add('r',
  'M114 45 L114 440',
  'M163 427 C206 459 254 464 326 464 C369 464 413 445 442 427',width=502)
add('u',
  'M107 442 L107 242 C107 101 170 25 291 25 C409 25 474 97 474 238 L474 442',
  'M474 182 L474 45',width=583)
add('f',
  'M207 43 L207 519 C207 632 257 683 391 683',
  'M91 434 L388 434',width=464)
add('t',
  'M218 624 L218 161 C218 75 259 36 352 36 C391 36 421 47 453 68',
  'M101 439 L427 439',width=509)
add('v',
  'M88 444 L235 104 C272 18 302 14 348 108 L486 444',width=563)
add('w',
  'M68 445 L165 108 C186 25 211 18 239 106 L303 297',
  'M315 297 L379 107 C408 18 431 25 452 109 L551 445',width=622)
add('x',
  'M106 443 C206 317 373 144 467 43',
  'M463 444 C365 316 203 143 107 43',width=577)
add('k',
  'M111 682 L111 43',
  'M450 444 C377 362 265 276 164 229',
  'M311 241 C384 156 429 101 480 43',width=570)
add('y',
  'M89 443 C149 299 209 161 284 49',
  'M480 443 C426 317 365 166 298 -15 C258 -123 221 -183 117 -183',width=568)
add('z',
  'M111 441 L451 441 C484 441 479 422 456 394 C362 291 236 171 125 96 C98 71 115 44 153 44 L476 44',width=579)
# Mirrored logo rhythm applied to numeral and a few structural punctuation glyphs.
add('0',
  'M294 663 L333 663 C460 663 527 534 527 342 C527 151 462 26 336 26 L290 26 C164 26 100 151 100 342 C100 534 169 663 294 663 Z',width=628)
add('2',
  'M122 520 C148 612 232 659 328 659 C451 659 520 585 520 493 C520 398 446 322 353 248 L141 87 C115 62 123 48 162 48 L536 48',width=626)
add('3',
  'M128 561 C174 628 248 661 329 661 C448 661 516 595 516 511 C516 423 444 359 325 359 L240 359',
  'M256 343 L342 343 C464 343 531 292 531 195 C531 96 449 24 327 24 C243 24 174 58 117 123',width=631)
add('5',
  'M515 638 L160 638 L147 387 C193 416 262 433 325 433 C442 433 521 349 521 229 C521 104 440 27 322 27 C225 27 165 66 114 121',width=625)
add('6',
  'M483 586 C428 649 375 660 305 660 C170 660 98 534 98 342 C98 156 177 23 316 23 C452 23 519 119 519 226 C519 346 436 413 322 413 C250 413 186 383 144 326',width=627)
add('9',
  'M143 96 C194 39 252 24 319 24 C460 24 527 156 527 343 C527 534 450 661 312 661 C177 661 102 566 102 464 C102 342 183 277 300 277 C375 277 437 310 482 364',width=628)
add('8',
  'M307 659 C429 659 491 598 491 512 C491 421 415 367 306 367 C197 367 128 421 128 512 C128 598 189 659 307 659 Z',
  'M307 344 C442 344 519 280 519 187 C519 90 441 24 307 24 C173 24 94 90 94 187 C94 280 171 344 307 344 Z',width=621)
# Soften square braces/slashes & graph signs in editor view.
add('[','M348 690 L160 690 Q151 690 151 677 L151 -150 Q151 -167 165 -167 L348 -167',width=438)
add(']','M93 690 L279 690 Q289 690 289 677 L289 -150 Q289 -167 275 -167 L93 -167',width=438)
# Use a uniquely curled at-sign, instead of circular off-the-shelf.
add('@',
  'M505 374 C505 439 459 482 387 482 C308 482 256 425 256 333 C256 245 305 202 378 202 C448 202 501 258 501 336 L501 463',
  'M503 279 C503 205 542 184 579 212 C623 244 626 311 626 377 C626 531 509 659 330 659 C154 659 74 534 74 344 C74 148 169 27 341 27 C453 27 529 72 592 127',width=719)

# Narrowing widths and alignment for natural texts.
for c in 'ilI|!.,:;\'`': W[c]=int(W[c])
G[' ']=[]; W[' ']=282
G['\u00a0']=[]; W['\u00a0']=282

# Diacritics placed clear of overshoots, are original curved strokes.
ACC={
'acute': 'M233 547 L338 696',
'grave': 'M238 696 L337 547',
'circ': 'M179 561 L278 675 L385 561',
'tilde': 'M176 603 C225 659 269 652 311 606 C352 560 391 565 426 608',
'dier': 'M188 610 L188 608;M382 610 L382 608',
'ring': 'M282 704 C327 704 356 671 356 629 C356 585 326 559 282 559 C239 559 207 585 207 629 C207 671 239 704 282 704 Z',
'cedilla':'M292 10 L274 -66 C347 -72 373 -99 364 -136 C351 -189 266 -185 233 -156',
'caron': 'M170 675 L278 560 L387 675',
'breve': 'M191 663 C200 570 357 569 371 663',
'dot': 'M282 633 L282 631',
'macron': 'M169 620 L394 620',
}
# For capitals place accents above cap line using shift up
UPPER_ACC={
'acute':'M233 727 L343 868', 'grave':'M228 868 L343 727',
'circ':'M178 731 L282 848 L392 731',
'&tilde':'M168 775 C215 830 263 820 303 773 C350 728 392 733 428 774',
'dier':'M183 787 L183 785;M386 787 L386 785',
'ring':'M282 883 C327 883 358 850 358 809 C358 766 327 738 282 738 C240 738 209 765 209 809 C209 851 240 883 282 883 Z',
'cedilla':ACC['cedilla'],
}
# derive marks adjusted for different base advance widths, path generated using path shifting.
latin_acc={
'À':('A','grave'),'Á':('A','acute'),'Â':('A','circ'),'Ã':('A','tilde'),'Ä':('A','dier'),'Å':('A','ring'),
'Ç':('C','cedilla'),'È':('E','grave'),'É':('E','acute'),'Ê':('E','circ'),'Ë':('E','dier'),
'Ì':('I','grave'),'Í':('I','acute'),'Î':('I','circ'),'Ï':('I','dier'),
'Ñ':('N','tilde'),'Ò':('O','grave'),'Ó':('O','acute'),'Ô':('O','circ'),'Õ':('O','tilde'),'Ö':('O','dier'),
'Ù':('U','grave'),'Ú':('U','acute'),'Û':('U','circ'),'Ü':('U','dier'),'Ý':('Y','acute'),
'à':('a','grave'),'á':('a','acute'),'â':('a','circ'),'ã':('a','tilde'),'ä':('a','dier'),'å':('a','ring'),
'ç':('c','cedilla'),'è':('e','grave'),'é':('e','acute'),'ê':('e','circ'),'ë':('e','dier'),
'ì':('i','grave'),'í':('i','acute'),'î':('i','circ'),'ï':('i','dier'),
'ñ':('n','tilde'),'ò':('o','grave'),'ó':('o','acute'),'ô':('o','circ'),'õ':('o','tilde'),'ö':('o','dier'),
'ù':('u','grave'),'ú':('u','acute'),'û':('u','circ'),'ü':('u','dier'),'ý':('y','acute'),'ÿ':('y','dier'),
'Š':('S','caron'),'š':('s','caron'),'Ž':('Z','caron'),'ž':('z','caron'),
'Č':('C','caron'),'č':('c','caron'),'Ř':('R','caron'),'ř':('r','caron'),
'Ł':('L',None),'ł':('l',None),
}
# Include S which stylistically mirrors logo, and lowercase alternates with full two strand flow.
# Cache sample point arrays as more robust than relying on stock typefaces.
pat_re = re.compile(r'([MLHVQCZmlhvqcz])|([-+]?(?:\d*\.\d+|\d+\.?\d*))')
def points(svg):
    chunks=[]
    for part in svg.split(';'):
        tok=[x or y for x,y in pat_re.findall(part)]
        j=0;p=(0.,0.);first=None; arr=[]
        while j<len(tok):
            cmd=tok[j];j+=1
            if cmd in 'Mm':
                p=(float(tok[j]),float(tok[j+1]));j+=2;first=p;arr.append(p)
            elif cmd in 'Ll':
                q=(float(tok[j]),float(tok[j+1]));j+=2; arr.append(q);p=q
            elif cmd in 'Hh':
                q=(float(tok[j]),p[1]);j+=1;arr.append(q);p=q
            elif cmd in 'Vv':
                q=(p[0],float(tok[j]));j+=1;arr.append(q);p=q
            elif cmd in 'Qq':
                a=(float(tok[j]),float(tok[j+1]));b=(float(tok[j+2]),float(tok[j+3]));j+=4
                n=max(12,min(45,int(math.dist(p,b)/7)))
                for i in range(1,n+1):
                    t=i/n;u=1-t;arr.append((u*u*p[0]+2*u*t*a[0]+t*t*b[0],u*u*p[1]+2*u*t*a[1]+t*t*b[1]))
                p=b
            elif cmd in 'Cc':
                a=(float(tok[j]),float(tok[j+1]));b=(float(tok[j+2]),float(tok[j+3]));c=(float(tok[j+4]),float(tok[j+5]));j+=6
                n=max(14,min(62,int((math.dist(p,a)+math.dist(a,b)+math.dist(b,c))/11)))
                for i in range(1,n+1):
                    t=i/n;u=1-t;arr.append((u**3*p[0]+3*u*u*t*a[0]+3*u*t*t*b[0]+t**3*c[0],u**3*p[1]+3*u*u*t*a[1]+3*u*t*t*b[1]+t**3*c[1]))
                p=c
            elif cmd in 'Zz':
                if arr and first:arr.append(first);p=first
            else:raise ValueError((cmd, svg))
        if len(arr)>1:chunks.append(arr)
    return chunks

# More precise accents: adjust prebuilt mark centered at 285 to glyph's middle.
base_pts={c:[v for p in paths for v in points(p)] for c,paths in G.items()}
for letter,(base,mark) in latin_acc.items():
    pths=copy.deepcopy(base_pts[base]);width=W[base]
    if mark:
        if mark=='cedilla':
            m=ACC[mark]
            mark_pts=points(m)
            # align below base bottom center, a slight 60-unit correction for C
            dx=(width-590)/2
            mark_pts=[[(x+dx,y) for x,y in line] for line in mark_pts]
        else:
            if base.isupper():
                if mark=='tilde':m=UPPER_ACC['&tilde']
                elif mark in UPPER_ACC:m=UPPER_ACC[mark]
                elif mark=='caron':m='M178 847 L283 731 L390 847'
                else:m=ACC[mark]
            else:m=ACC[mark]
            mark_pts=points(m)
            dx=width/2-284
            # dot of lowercase i would overlap accent; replace that dot
            if base in ('i','j'):
                pths=[line for line in pths if not (max(y for x,y in line)>600)]
            mark_pts=[[(x+dx,y) for x,y in line] for line in mark_pts]
        pths+=mark_pts
    else:
        pths+=points('M126 265 L287 384')
    base_pts[letter]=pths;W[letter]=width
# additional common Latin characters from own curves
for ch,aspaths,ww in [
('Æ',['M73 45 L246 635 L585 635','M148 267 L504 267','M246 635 L246 52 L595 52'],678),
('æ',['M462 240 C460 369 414 461 296 461 C175 461 105 363 105 241 C105 110 176 24 290 24 C394 24 449 110 449 240','M441 268 L670 268 C668 379 623 461 547 461 C459 461 437 396 437 278 C437 123 493 25 586 25 C647 25 680 58 713 102'],806),
('Ø',['M320 663 C464 663 547 545 547 346 C547 143 464 27 320 27 C176 27 93 143 93 346 C93 545 176 663 320 663 Z','M67 -2 L564 697'],641),
('ø',['M290 465 C416 465 485 370 485 246 C485 115 416 23 290 23 C165 23 96 115 96 246 C96 370 165 465 290 465 Z','M86 -25 L500 494'],580),
('ß',['M114 50 L114 471 C114 602 176 687 301 687 C422 687 480 609 480 520 C480 440 430 390 368 345 C477 312 526 247 526 169 C526 73 465 23 348 23 C302 23 262 37 227 55'],594),
('Ð',['M117 50 L117 634','M153 635 L318 635 C493 635 544 521 544 345 C544 169 488 56 313 56 L154 56','M66 343 L375 343'],644),
('ð',['M123 222 C123 352 188 456 294 456 C415 456 480 350 480 223 C480 100 414 23 293 23 C186 23 123 100 123 222 Z','M447 642 L192 517','M217 681 L461 480'],578),
('Þ',['M112 50 L112 635','M151 525 L326 525 C462 525 523 461 523 352 C523 245 461 179 326 179 L151 179'],637),
('þ',['M111 -181 L111 675','M125 236 C125 359 185 462 301 462 C416 462 492 373 492 238 C492 106 416 24 301 24 C185 24 125 113 125 236 Z'],580),
]:
    base_pts[ch]=[line for p in aspaths for line in points(p)];W[ch]=ww
# For special copyright, registered, trade marks reusing original shapes
base_pts['©']=points('M351 661 C512 661 628 540 628 342 C628 143 512 27 351 27 C187 27 75 143 75 342 C75 540 187 661 351 661 Z')+[[(x+55,y) for x,y in p] for p in base_pts['c']];W['©']=706
base_pts['®']=points('M351 661 C512 661 628 540 628 342 C628 143 512 27 351 27 C187 27 75 143 75 342 C75 540 187 661 351 661 Z')+points('M261 181 L261 493;M269 493 L357 493 C438 493 466 456 466 397 C466 337 431 310 360 310 L270 310;M364 308 L472 181');W['®']=706
base_pts['™']=points('M70 623 L256 623;M164 613 L164 337;M328 339 L328 617 L430 451 L531 617 L531 339');W['™']=626
# special dotted i increase visual clarity: set dot filled via segment buffered to circle width diameter
# LSB extra space from explicit stroke positive coordinates

def poly_shapes(lines, stroke):
    shapes=[]
    for pts in lines:
        if len(pts)<2:continue
        # Drop consecutive duplicates
        pts2=[pts[0]]
        for t in pts[1:]:
            if math.dist(t,pts2[-1])>0.01:pts2.append(t)
        if len(pts2)<2:continue
        geom=LineString(pts2).buffer(stroke/2,resolution=14,cap_style=1,join_style=1)
        shapes.append(geom)
    if not shapes:return []
    united=unary_union(shapes).buffer(0)
    if united.geom_type=='Polygon':return [united]
    if united.geom_type=='MultiPolygon':return list(united.geoms)
    return [p for p in united.geoms if p.geom_type=='Polygon']

def buildglyph(lines,stroke):
    pen=TTGlyphPen(None)
    for p in poly_shapes(lines,stroke):
        p=orient(p,sign=-1)
        for ring in [p.exterior,*p.interiors]:
            # shapely buffer generated many points -> simplify with tiny tolerance maintaining smooth shape
            r=ring.simplify(0.85, preserve_topology=True)
            coords=list(r.coords)[:-1]
            if len(coords)<3:continue
            pen.moveTo(tuple(map(lambda n:round(n),coords[0])))
            for pt in coords[1:]:pen.lineTo(tuple(map(lambda n:round(n),pt)))
            pen.closePath()
    return pen.glyph()

# Font metrics
FONT_FAMILY='Seloryn Font'
EXCLUDE={'\u00a0'}
# Exactly English + Brazilian Portuguese letters, common typography, money,
# and basic programming punctuation (Seloryn is a programming language).
PTBR_ACCENTS = 'ÁÀÂÃÉÊÍÓÔÕÚÇáàâãéêíóôõúç'
TYPOGRAPHY = '‘’“”ªº·−°–—…•©®™€£'
COVERAGE = {chr(i) for i in range(32,127)} | set(PTBR_ACCENTS+TYPOGRAPHY+'\u00a0')
chars=sorted((c for c in base_pts if c in COVERAGE),key=ord)
assert COVERAGE == set(chars), 'Missing expected English/PT-BR glyphs: '+repr(COVERAGE-set(chars))
notdef=points('M90 0 L90 690 L520 690 L520 0 Z;M90 0 L520 690')

kern_pairs={
'AV':-32,'AW':-23,'AY':-40,'AT':-14,'TA':-35,'To':-26,'Te':-20,'Tr':-16,'Tu':-14,
'VA':-37,'WA':-29,'YA':-44,'Yo':-28,'Ye':-23,'LY':-34,'LT':-20,'LO':-15,'Ty':-24,
'Fa':-19,'Fo':-16,'PA':-32,'Wa':-20,'Va':-26,'Wo':-18,'Vo':-23,
'Se':-10,'Sy':-7,'ry':-12,'ro':-8,'ra':-7,'ly':-7,'yo':-10,'ov':-8,'ow':-8,
'Ve':-18,'Av':-16,'Ax':-12,'Ta':-20,'Ye':-21,
}

# Accent-aware kerning for English and Brazilian Portuguese.
for pair,val in list(kern_pairs.items()):
    for accent_char,(base,mark) in latin_acc.items():
        if accent_char not in COVERAGE: continue
        if pair[0] == base and pair[1] in COVERAGE:
            kern_pairs.setdefault(accent_char+pair[1],val)
        if pair[1] == base and pair[0] in COVERAGE:
            kern_pairs.setdefault(pair[0]+accent_char,val)

def build(weight, stroke, slug):
    mapping={c:'uni%04X'%ord(c) if ord(c)<=0xFFFF else 'u%06X'%ord(c) for c in chars}
    order=['.notdef','space']+[mapping[c] for c in chars if c!=' ']
    glyphs={'.notdef':buildglyph(notdef,stroke)}
    hmtx={'.notdef':(650,40)}
    for c in chars:
        name='space' if c==' ' else mapping[c]
        if name in glyphs:continue
        lines=base_pts[c]
        glyphs[name]=buildglyph(lines,stroke)
        coords=[(x,y) for line in lines for x,y in line]
        xmin=min((x for x,y in coords),default=0)-stroke/2
        # single note: width should be enough for furthest right point + bearing
        xmax=max((x for x,y in coords),default=0)+stroke/2
        advance=max(W[c],round(xmax+50)) if c not in 'ilI!' else max(W[c],round(xmax+45))
        hmtx[name]=(int(advance),round(xmin) if coords else 0)
    fb=FontBuilder(1000,isTTF=True)
    fb.setupGlyphOrder(order)
    cmap={ord(c):('space' if c==' ' else mapping[c]) for c in chars}
    fb.setupCharacterMap(cmap)
    fb.setupGlyf(glyphs)
    fb.setupHorizontalMetrics(hmtx)
    fb.setupHorizontalHeader(ascent=890,descent=-225,lineGap=35)
    fb.setupNameTable({'familyName':FONT_FAMILY,'styleName':weight,'uniqueFontIdentifier':'SelorynFont-'+slug+'-v2.0',
        'fullName':FONT_FAMILY+' '+weight,'psName':'SelorynFont-'+slug,'version':'Version 2.000',
        'copyright':'Copyright (c) 2026 Seloryn contributors. MIT License.',
        'description':'Seloryn Font v2.0, modular open-bowl letterforms based on the original split-flow S mark.',
        'designer':'Seloryn custom type design'})
    fb.setupOS2(sTypoAscender=890,sTypoDescender=-225,sTypoLineGap=35,usWinAscent=940,usWinDescent=235,
        sxHeight=480,sCapHeight=692,usWeightClass=(300 if weight=='Light' else 700),
        usWidthClass=5,fsSelection=0,panose={'bFamilyType':2,'bSerifStyle':11,'bWeight':(3 if weight=='Light' else 8),
        'bProportion':3,'bContrast':2,'bStrokeVariation':2,'bArmStyle':3,'bLetterForm':2,'bMidline':3,'bXHeight':4})
    fb.setupPost()
    fb.setupMaxp()
    fb.setupDummyDSIG()
    font=fb.font
    # Generated GPOS kerning from manually reviewed optical pairs.
    fea='languagesystem DFLT dflt;\nlanguagesystem latn dflt;\nfeature kern {\n'
    for pair,val in kern_pairs.items():
        if pair[0] in mapping and pair[1] in mapping:
            fea+=f' pos {mapping[pair[0]]} {mapping[pair[1]]} {val};\n'
    fea+='} kern;'
    addOpenTypeFeaturesFromString(font,fea)
    p=OUT/f'seloryn-font-{slug.lower()}.ttf';font.save(str(p),reorderTables=True)
    font.flavor='woff2'; font.save(str(OUT/f'seloryn-font-{slug.lower()}.woff2'),reorderTables=True)
    font.flavor='woff'; font.save(str(OUT/f'seloryn-font-{slug.lower()}.woff'),reorderTables=True)
    print(weight,len(chars),'glyphs',p.stat().st_size,'woff2',(OUT/f'seloryn-font-{slug.lower()}.woff2').stat().st_size)

build('Light',55,'Light')
build('Bold',113,'Bold')
print('glyph char count:',len(chars))
