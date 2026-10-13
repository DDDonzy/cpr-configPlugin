from pathlib import Path
import base64,hashlib,json
root=Path(__file__).resolve().parent.parent/'codex-setup'
dll=root/'vendor/lib/net40/Nett.dll'
b=dll.read_bytes();sha=hashlib.sha256(b).hexdigest()
s=(root/'client/setup.ps1').read_text(encoding='utf-8-sig').replace('__NETT_SHA256__',sha).replace('__NETT_BASE64__',base64.b64encode(b).decode('ascii'))
(root/'dist').mkdir(exist_ok=True)
(root/'dist/setup.runtime.ps1').write_text(s,encoding='utf-8-sig')
header=r'''@echo off
setlocal
set "CPR_SETUP_SELF=%~f0"
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -Command "$s=[IO.File]::ReadAllText($env:CPR_SETUP_SELF,[Text.Encoding]::UTF8);$m=[regex]::Match($s,'(?m)^#--CPR-POWERSHELL--\r?$');if(!$m.Success){exit 1}; & ([scriptblock]::Create($s.Substring($m.Index+$m.Length)))"
set "CPR_SETUP_RC=%ERRORLEVEL%"
if not "%CPR_SETUP_RC%"=="0" pause
exit /b %CPR_SETUP_RC%
#--CPR-POWERSHELL--
'''
bat=(header+s).replace('\r\n','\n').replace('\n','\r\n').encode('utf-8')
(root/'dist/codex-setup.bat').write_bytes(bat)
report={'version':'1.0.0','nett_dll_sha256':sha,'bat_sha256':hashlib.sha256(bat).hexdigest(),'bat_bytes':len(bat),'nett_version':'0.15.0','nett_target':'net40','license':'MIT'}
(root/'dist/build.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
repo=root.parent
(repo/'dist').mkdir(exist_ok=True)
(repo/'dist/codex-setup.bat').write_bytes(bat)
(repo/'codex-setup/server/assets').mkdir(exist_ok=True)
(repo/'codex-setup/server/assets/codex-setup.bat').write_bytes(bat)
print(json.dumps(report,indent=2))
