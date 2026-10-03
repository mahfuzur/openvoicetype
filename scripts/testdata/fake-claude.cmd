@echo off
rem The fake claude CLI (fake-claude, a bash script) for a native Windows program under test, run by Git Bash.
"%ProgramFiles%\Git\bin\bash.exe" "%~dp0fake-claude" %*
