# SubForge 0.1.6

Sidos - info@sidos.gr

## English

This repository publishes the application source, pinned dependency lockfiles,
Windows build scripts, icons and ten offline HTML guides.

Version 0.1.6 repairs local Whisper subtitle cues whose end extends beyond an
audio part, retaining their text, ID and start. It also records original/repaired
timings and detailed failure-stage context in per-file logs.

Validation: 36 Rust tests, five focused desktop checks and successful local
recognition of the reported failing 300-second audio part. No episode audio or
dialogue was uploaded in that validation. Full-episode Greek translation and
clean Windows 10/11 virtual-machine installation are not claimed by these checks.

The existing Windows installer is a local evaluation build. Its bundled static
FFmpeg distribution includes GPL dependencies whose complete corresponding-source
package has not yet been assembled. That installer is not attached to this public
repository or published as a public release asset. The application and installer
are unsigned. See `README.md` for local build instructions and `BUILD_REPORT.md`
for the detailed validation record.

No SubForge source-code license has been selected. Third-party components retain
their original copyright and licenses; see `src-tauri/resources/licenses`.

## Ελληνικά

Το αποθετήριο δημοσιεύει τον πηγαίο κώδικα της εφαρμογής, κλειδωμένες εκδόσεις
εξαρτήσεων, εργαλεία δημιουργίας για Windows, εικονίδια και δέκα οδηγούς HTML.

Η έκδοση 0.1.6 διορθώνει υποτίτλους τοπικής αναγνώρισης Whisper των οποίων η λήξη
ξεπερνά το τμήμα ήχου, διατηρώντας κείμενο, αναγνωριστικό και αρχή. Καταγράφει
επίσης τους αρχικούς/διορθωμένους χρονισμούς και το στάδιο αποτυχίας ανά αρχείο.

Έλεγχοι: 36 δοκιμές Rust, πέντε στοχευμένοι έλεγχοι διεπαφής και επιτυχής τοπική
αναγνώριση του προβληματικού τμήματος ήχου διάρκειας 300 δευτερολέπτων.
Δεν μεταφορτώθηκαν ήχος ή διάλογοι επεισοδίου κατά τον έλεγχο. Οι δοκιμές αυτές
δεν πιστοποιούν πλήρη ελληνική μετάφραση επεισοδίου ή εγκατάσταση σε καθαρές
εικονικές μηχανές Windows 10/11.

Ο υπάρχων εγκαταστάτης προορίζεται για τοπική αξιολόγηση. Η στατική διανομή FFmpeg
που περιλαμβάνει έχει εξαρτήσεις GPL για τις οποίες δεν έχει συγκεντρωθεί ακόμη
πλήρες πακέτο αντίστοιχου πηγαίου κώδικα. Για αυτό ο εγκαταστάτης δεν επισυνάπτεται
στο δημόσιο αποθετήριο ή σε δημόσια έκδοση. Η εφαρμογή και ο εγκαταστάτης δεν
διαθέτουν ψηφιακή υπογραφή. Οδηγίες τοπικής δημιουργίας υπάρχουν στο `README.md`
και αναλυτικά αποτελέσματα στο `BUILD_REPORT.md`.

Δεν έχει επιλεγεί άδεια για τον κώδικα του SubForge. Οι εξαρτήσεις διατηρούν τις
δικές τους άδειες· δείτε τον φάκελο `src-tauri/resources/licenses`.
