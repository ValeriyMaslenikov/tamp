; tamp NSIS installer hooks.
;
; NOTE (not built locally — verify on the next real Windows release):
; This .nsh is compiled only during `tauri build` on Windows; it is reviewed by
; hand here. After an uninstall on a machine where "Launch at login" was on,
; manually confirm:
;   * HKCU\Software\Microsoft\Windows\CurrentVersion\Run has NO "tamp" value
;     (and the matching StartupApproved\Run "tamp" value is gone) — no dead
;     Run entry pointing at the removed exe.
;   * %LOCALAPPDATA%\io.github.valeriymaslenikov.tamp\{thumbs,previews,logs} and
;     durations.json are gone (transient caches/logs reclaimed).
;   * %APPDATA%\io.github.valeriymaslenikov.tamp\{settings.json,conversions.json}
;     are PRESERVED (settings + conversion history survive for reinstall).
; Every step below is best-effort: NSIS DeleteRegValue / RMDir silently no-op
; when the target is already absent, so a partial install never blocks removal.

!macro NSIS_HOOK_PREUNINSTALL
  ; Remove the per-user "Compress with tamp" Explorer entries the app registered.
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.mov\shell\tamp.compress"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.mp4\shell\tamp.compress"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.m4v\shell\tamp.compress"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.webm\shell\tamp.compress"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.mkv\shell\tamp.compress"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.avi\shell\tamp.compress"

  ; Remove the launch-at-login autostart entry the autostart plugin writes when
  ; "Launch at login" is enabled. The plugin (auto-launch) keys the value by the
  ; package name "tamp" (tamp passes no override), writing both the Run value and
  ; a StartupApproved\Run override; otherwise an uninstall leaves a dead Run entry
  ; pointing at the removed exe. DeleteRegValue is a no-op if the value is absent.
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "tamp"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "tamp"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Reclaim transient caches/logs left under the per-identifier LOCAL app-data
  ; dir (%LOCALAPPDATA%\io.github.valeriymaslenikov.tamp) — thumbnails, prepared
  ; previews, the per-file duration cache, and rotating logs. These are all
  ; rebuildable and worthless once the app is gone. RMDir /r is best-effort and
  ; silently no-ops on a missing path.
  ;
  ; We deliberately DO NOT touch the ROAMING app-data dir
  ; (%APPDATA%\io.github.valeriymaslenikov.tamp), which holds settings.json and
  ; the conversions.json history — kept so a reinstall picks up the user's
  ; presets and Converted-tab history (reinstall continuity).
  RMDir /r "$LOCALAPPDATA\io.github.valeriymaslenikov.tamp\thumbs"
  RMDir /r "$LOCALAPPDATA\io.github.valeriymaslenikov.tamp\previews"
  RMDir /r "$LOCALAPPDATA\io.github.valeriymaslenikov.tamp\logs"
  Delete "$LOCALAPPDATA\io.github.valeriymaslenikov.tamp\durations.json"
  ; Drop the local app-data dir itself only if now empty (RMDir without /r fails
  ; harmlessly when anything remains, so user data elsewhere is never at risk).
  RMDir "$LOCALAPPDATA\io.github.valeriymaslenikov.tamp"
!macroend
