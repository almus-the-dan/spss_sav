* Encoding real-file test fixture generator (UTF-8).
* Regenerate with:  pspp encoding_utf8.sps
* Declares UTF-8 in both the character encoding record (type 7
* subtype 20, payload "UTF-8") and the machine integer info record
* (type 7 subtype 3, character_code 65001), and puts non-ASCII text in
* every distinct decode path: the header file label, a variable label,
* value labels, a document line, and a file attribute value.
* Subtype 20 is written LAST, immediately before the 999 terminator,
* which is the whole reason string decoding must be deferred.
* NOTE: the file's creation timestamp and the DOCUMENT "(Entered ...)"
* line depend on generation time; tests must not assert on those.

DATA LIST LIST /id (F2.0) prenom (A20).
BEGIN DATA
1 "café"
2 "naïve"
END DATA.

FILE LABEL 'Fichier de démonstration'.
VARIABLE LABELS id 'Identifiant' prenom 'Prénom accentué'.
VALUE LABELS id 1 'Café crème' 2 'Thé glacé'.
DATAFILE ATTRIBUTE ATTRIBUTE=Auteur('Ångström').

DOCUMENT Une ligne documentaire accentuée.

SAVE OUTFILE='encoding_utf8.sav'.
