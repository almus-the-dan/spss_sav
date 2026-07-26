* Encoding real-file test fixture generator (windows-1252).
* Regenerate with:  pspp encoding_windows1252.sps
* Byte-for-byte the same text as encoding_utf8.sps, but SET LOCALE
* makes PSPP declare windows-1252 in both the character encoding
* record (type 7 subtype 20, payload "WINDOWS-1252") and the machine
* integer info record (type 7 subtype 3, character_code 1252), and
* write the text as single-byte windows-1252 rather than UTF-8.
* This is the case where the file's declaration happens to agree with
* the reader's default fallback, so it guards against a regression in
* the agreeing path.
* NOTE: the file's creation timestamp and the DOCUMENT "(Entered ...)"
* line depend on generation time; tests must not assert on those.

SET LOCALE='windows-1252'.

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

SAVE OUTFILE='encoding_windows1252.sav'.
