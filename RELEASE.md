# SubForge 0.1.11

Sidos - info@sidos.gr

## English

- Saving settings now shows a persistent, visible confirmation, including after saving a provider.
- Info icons explain settings and provider options on hover or keyboard focus. Explanations are available in English and Greek, with English fallback for other interface languages.
- Translation prompts require the selected target language and its native writing system. Per-cue character checks reject stray foreign scripts, emoji, pictograms and replacement characters before checkpoints or final subtitles are saved. Existing cached parts are revalidated.
- Healthy translation and speech requests have no scheduled pauses, including with older profiles. The former minimum-pause and provider-delay controls have been removed.
- HTTP 429 and recognized agent quota errors immediately try another eligible enabled provider. If every route is limited, short adaptive backoffs start at one second. Server-requested retry times are retained; temporary learned spacing clears as requests succeed.
- Parallel translation parts retain independent checkpoints, cue order and timestamps. Pause, resume, retry, cancellation and live file logs remain available.

Also includes the changes since the public 0.1.6 source: OpenAI (ChatGPT) sign-in and account model selection, verified GitHub app updates, browser-launch and streamed-response fixes, stronger language validation, and plain subtitle formatting without embedded font sizes.

Validation: 69 Rust tests passed (two interactive/live-account tests skipped), 16 browser scenarios and 9 isolated native desktop checks passed. Frontend compilation and all ten language resource checks passed. Tests use synthetic fixtures; they do not establish live provider quotas or translation accuracy for every language. A clean-machine installation/upgrade was not exercised for this release.

Download SubForge-Setup-x64.exe and verify it using SHA256SUMS.txt. The installer is unsigned. The existing bundled FFmpeg corresponding-source packaging limitation is recorded in BUILD_REPORT.md and the third-party notices; no new licensing clearance is claimed by this release.

## Ελληνικά

- Η αποθήκευση ρυθμίσεων εμφανίζει σταθερή, ορατή επιβεβαίωση, ακόμη και μετά την αποθήκευση παρόχου.
- Εικονίδια πληροφοριών εξηγούν τις ρυθμίσεις με κατάδειξη ή εστίαση πληκτρολογίου. Υπάρχουν ελληνικές και αγγλικές εξηγήσεις, με αγγλική εφεδρεία για τις υπόλοιπες γλώσσες διεπαφής.
- Οι οδηγίες μετάφρασης απαιτούν τη γλώσσα στόχου και τη φυσική γραφή της. Έλεγχοι ανά υπότιτλο απορρίπτουν ξένες γραφές, emoji, εικονίδια και χαρακτήρες αντικατάστασης πριν από την αποθήκευση. Επανελέγχονται και τα αποθηκευμένα τμήματα.
- Τα επιτυχή αιτήματα μετάφρασης και αναγνώρισης δεν έχουν προγραμματισμένες παύσεις, ακόμη και με παλαιότερα προφίλ. Αφαιρέθηκαν οι ρυθμίσεις σταθερής παύσης.
- Τα HTTP 429 και τα αναγνωρισμένα όρια πρακτόρων ενεργοποιούν αμέσως άλλον διαθέσιμο ενεργό πάροχο. Αν όλοι έχουν όριο, οι προσαρμοστικές αναμονές ξεκινούν από ένα δευτερόλεπτο. Τηρούνται οι χρόνοι που ζητά ο διακομιστής.
- Τα παράλληλα τμήματα κρατούν ανεξάρτητα σημεία επαναφοράς, σειρά και χρόνους. Διατηρούνται παύση, συνέχιση, επανάληψη, ακύρωση και ζωντανές καταγραφές.

Περιλαμβάνονται επίσης οι αλλαγές μετά τη δημόσια 0.1.6: σύνδεση OpenAI (ChatGPT), επιλογή μοντέλων λογαριασμού, επαληθευμένες ενημερώσεις GitHub, διορθώσεις σύνδεσης και απαντήσεων ροής, αυστηρότεροι έλεγχοι γλώσσας και καθαροί υπότιτλοι χωρίς ενσωματωμένα μεγέθη γραμματοσειρών.

Πέρασαν 69 δοκιμές Rust, 16 σενάρια περιηγητή, 9 απομονωμένοι έλεγχοι εφαρμογής, μεταγλώττιση και οι δέκα γλώσσες. Παραλείφθηκαν δύο διαδραστικές δοκιμές/δοκιμές πραγματικού λογαριασμού. Οι δοκιμές δεν εγγυώνται όρια πραγματικών παρόχων ή ακρίβεια κάθε γλώσσας. Δεν έγινε δοκιμή εγκατάστασης/αναβάθμισης σε καθαρό υπολογιστή για αυτή την έκδοση.

Κατεβάστε το SubForge-Setup-x64.exe και ελέγξτε το με το SHA256SUMS.txt. Ο εγκαταστάτης δεν έχει ψηφιακή υπογραφή. Η υφιστάμενη εκκρεμότητα αντίστοιχου πηγαίου κώδικα FFmpeg καταγράφεται στο BUILD_REPORT.md και στις ειδοποιήσεις τρίτων.
