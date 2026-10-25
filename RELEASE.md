# SubForge 0.1.12

Sidos - info@sidos.gr

## English

Fixes subtitle creation failing after 0.1.11 rejected mixed-script provider replies and copied music-note markers.

- Valid lines are kept while only rejected lines are repaired. The repair request edits the translated text itself, rather than repeating the original source dialogue.
- Known music-note delimiters are removed while lyrics, cue IDs and timestamps are preserved. Foreign letters, emoji and unknown glyphs still require correction; they are never silently deleted from dialogue.
- Responses providers receive structured output requests and low-temperature generation when supported. Gateway format compatibility is learned per account/model, retaining validated translation for older gateways.
- Waiting for a cooldown extended by another parallel part no longer consumes request retries. Actual rate-limit failures still use configured retry limits and eligible enabled fallback providers.
- OpenAI stream failures retain bounded error codes for diagnosis; rate-limit events enter the shared cooldown/fallback path.

Validation: 72 Rust tests and five isolated native regression scenarios passed. Full local-file translation and timing checks passed, along with an opt-in synthetic OAuth translation. Two interactive/live-account tests are skipped in unattended CI.

The installer is unsigned. The existing bundled FFmpeg corresponding-source packaging limitation remains documented in BUILD_REPORT.md and the third-party notices.

## Ελληνικά

Διορθώνεται η αποτυχία δημιουργίας υποτίτλων μετά την 0.1.11, όταν απαντήσεις παρόχων περιείχαν ανάμεικτες γραφές ή αντιγραμμένα σύμβολα μουσικής.

- Διατηρούνται οι έγκυρες γραμμές και διορθώνονται μόνο οι απορριφθείσες. Η διόρθωση επεξεργάζεται το μεταφρασμένο κείμενο αντί να επαναλαμβάνει το αρχικό.
- Αφαιρούνται γνωστά μουσικά σύμβολα χωρίς απώλεια στίχων, αναγνωριστικών ή χρόνων. Οι ξένοι χαρακτήρες και τα άγνωστα εικονίδια εξακολουθούν να απαιτούν διόρθωση.
- Οι πάροχοι Responses χρησιμοποιούν δομημένες απαντήσεις και χαμηλή τυχαιότητα όπου υποστηρίζονται, με αυτόματη προσαρμογή σε παλαιότερες συμβατές πύλες.
- Η αναμονή κοινού ορίου από παράλληλα τμήματα δεν εξαντλεί τις προσπάθειες αιτημάτων. Διατηρούνται οι ρυθμισμένες επαναλήψεις και οι ενεργοί εναλλακτικοί πάροχοι.
- Τα σφάλματα ροής OpenAI εμφανίζουν ασφαλείς κωδικούς για διάγνωση και αναγνωρίζονται τα όρια αιτημάτων.

Ο εγκαταστάτης δεν έχει ψηφιακή υπογραφή. Η υφιστάμενη εκκρεμότητα αντίστοιχου πηγαίου κώδικα FFmpeg παραμένει καταγεγραμμένη στο BUILD_REPORT.md και στις ειδοποιήσεις τρίτων.
