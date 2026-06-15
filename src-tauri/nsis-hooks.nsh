!macro NSIS_HOOK_PREUNINSTALL
  ; Remove the per-user "Compress with tamp" Explorer entries the app registered.
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.mov\shell\tamp.compress"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.mp4\shell\tamp.compress"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.m4v\shell\tamp.compress"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.webm\shell\tamp.compress"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.mkv\shell\tamp.compress"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.avi\shell\tamp.compress"
!macroend
