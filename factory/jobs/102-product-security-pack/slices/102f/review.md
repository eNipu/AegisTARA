VERDICT: PASS
## For the owner
What changed: Japanese chapters 4 (logging), 5 (personal data, GDPR and APPI) and 6 (71 questionnaire answers) were added, so the pack is complete in both languages.
CI now runs the pack check and its test, so English and Japanese cannot drift apart in structure. ADR 0069 records the decisions.
README, the release checklist, gap G39 and the roadmap table now point to the pack.
Look at:
- docs/security-pack/ja/05-personal-data.md:5-7 and :83-95. These are the legal wording: the "not legal advice" sentence, the full names of GDPR and APPI, and the APPI duty terms. The prompt asks for a legal review of chapter 5 before the pack goes to a customer, and this is where to start.
- docs/security-pack/ja/04-logging-and-retention.md:49,52 and ja/06-security-questionnaire.md:61,118. "head hash" is translated as 先頭のハッシュ. In Japanese 先頭 can read as the *first* (oldest) entry, but the chain head is the *latest* one. The text says "the hash the check shows", so a customer would still copy the right value. The native-speaker reader should still decide between 先頭 and a term such as 最新のハッシュ or チェーンヘッド.
- docs/security-pack/ja/06-security-questionnaire.md:34 (Q-11). 空白だけでなく can read as "not only spaces but also …". The meaning is "must not be only spaces". 空白だけではなく or 空白のみではなく would be clearer. Give this to the native-speaker reader.
Risk: Low. The change is documentation plus two CI steps. Both scripts are executable (mode 100755), and the new steps match the offline-bundle step in style and indentation. The gate passed: the pack check reports 12 files ok and the script test passes. I read all three translations line by line against the English. They match section by section and row by row: nothing was added, dropped or softened, and the article numbers are unchanged. The status words are translated as the prompt says, and the 出典 (sources) lists are identical to the English.
View: git -C factory/worktrees/102-product-security-pack diff b138e57debd0da2264b6916033058cbc9d6c7ec9..HEAD
## Findings
- None blocking.
- Done-when 5 (source-bullet diff) is not met as written, and this is not a defect in the slice. The prompt's pattern `^- \`` also matches body bullets that start with a backticked word: RUST_LOG in 4.2, AEGIS_EVENT_RETENTION_DAYS in 5.3, and the six status-word bullets in 6.1. Those lines are translated on purpose. I compared the 出典 sections by eye and they are identical, and the script's R4 and R5 rules pass. The same thing happened in 102e.
- The two wording points under "Look at" are for the planned native-speaker read, not for the builder.
## Lesson
[planner] When a done-when check greps for a line shape, such as source bullets, limit it to the section it means (for example, the lines after the Sources heading). Otherwise it also matches body lines that are translated on purpose and fails for no reason. This happened in 102e and again in 102f.
NEXT: human
