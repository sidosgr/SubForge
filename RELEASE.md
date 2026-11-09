# SubForge 0.1.13

## English

**Proof of concept for testing purposes only.** Live provider testing has covered Google Antigravity as an agent, OpenCode CLI as an agent, OpenRouter, and ChatGPT OAuth.

- Settings now has six tabs. The AI Providers tab opens with the translation and speech provider lists.
- OpenCode Zen and the embedded OpenCode Console route have been removed. Existing settings and queued jobs drop retired routes when loaded.
- Saving a new OpenCode CLI provider opens the managed CLI v2 login terminal. Model discovery and translation use its active OAuth session.
- Settings save confirmations disappear after five seconds. Queued and resumed jobs check the current provider list.

The Windows installer is unsigned. See [the build report](BUILD_REPORT.md) for packaging and validation details.

## Ελληνικά

**Πειραματικό πρωτότυπο αποκλειστικά για δοκιμές.** Δοκιμές με πραγματικούς παρόχους έγιναν με το Google Antigravity ως πράκτορα, το OpenCode CLI ως πράκτορα, το OpenRouter και το ChatGPT μέσω OAuth.

- Οι Ρυθμίσεις έχουν πλέον έξι καρτέλες. Στην καρτέλα **Πάροχοι ΤΝ** εμφανίζονται πρώτα οι λίστες μετάφρασης και απομαγνητοφώνησης.
- Καταργήθηκαν το OpenCode Zen και η ενσωματωμένη σύνδεση OpenCode Console. Οι καταργημένοι πάροχοι αφαιρούνται κατά τη φόρτωση ρυθμίσεων και εργασιών.
- Η αποθήκευση νέου παρόχου OpenCode CLI ανοίγει το τερματικό σύνδεσης του διαχειριζόμενου CLI v2. Η εύρεση μοντέλων και η μετάφραση χρησιμοποιούν την ενεργή σύνδεση OAuth.
- Η επιβεβαίωση αποθήκευσης κλείνει μετά από πέντε δευτερόλεπτα. Οι εργασίες στην ουρά και όσες συνεχίζονται ελέγχουν την τρέχουσα λίστα παρόχων.

Ο εγκαταστάτης Windows δεν έχει ψηφιακή υπογραφή. Δείτε την [αναφορά ελέγχων](BUILD_REPORT.md) για λεπτομέρειες.
