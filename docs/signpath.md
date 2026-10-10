# SignPath Foundation: Checkliste

Bedingungen: https://signpath.org/terms · Antrag: https://signpath.org/apply

## Vor dem Antrag

- [x] Repo `0xshrd/utilix-tools` ist öffentlich, `LICENSE` ist GPLv3, keine proprietären Bestandteile.
- [ ] Mindestens ein Tool funktioniert und ist als GitHub-Release veröffentlicht (unsigniert).
- [ ] Nachweisbare Reputation: Downloads, Sterne, Erwähnungen. Ohne die lehnt SignPath ausführbare Programme ab.
- [ ] Funktionsbeschreibung auf der Download-Seite (README und Website).
- [x] Zwei-Faktor-Authentifizierung für alle Mitglieder von `0xshrd` aktiv, in der Organisation erzwungen.
- [ ] Branch-Schutz für `main`: Pull Requests von Externen brauchen ein Review.
- [x] `Cargo.lock` committet, damit Builds reproduzierbar sind.
- [ ] Keine Funktionen, die Schwachstellen suchen/ausnutzen oder Sicherheitsmaßnahmen umgehen.

## Website (0xshrd/utilix)

- [ ] In `src/data/site.ts` `codeSigningPolicy` auf `true` setzen, sobald die Zusage da ist.
      Dann erscheinen Abschnitt „Code signing policy“, SignPath-Hinweis, Rollen und Datenschutzaussage auf der Startseite.

## Team

- Aaron ([grrtbow](https://github.com/grrtbow)) und Tom ([markmarkinson](https://github.com/markmarkinson)): Committer, Reviewer und Approver (beide Owner von `0xshrd`).

## Nach der Zusage

1. In SignPath: Projekt `utilix-tools` und Signing Policy `release-signing` anlegen,
   GitHub als Trusted Build System verbinden.
2. Metadaten-Regeln setzen: Product name = `utilix`, Product version für alle Dateien gleich.
3. Im GitHub-Repo:
   - Secret `SIGNPATH_API_TOKEN`
   - Variable `SIGNPATH_ORGANIZATION_ID`
   - Variable `SIGNPATH_ENABLED` = `true`
4. Im README den SignPath-Satz unter „Code signing policy“ einkommentieren.
5. Release taggen (`git tag v0.x.y && git push --tags`), Signieranfrage in SignPath freigeben.
