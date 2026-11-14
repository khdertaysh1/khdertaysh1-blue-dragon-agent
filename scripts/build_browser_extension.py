"""Package independently written Firefox/Chromium extension variants (unsigned)."""
from pathlib import Path
import json
import shutil
import zipfile

base=Path(__file__).resolve().parent.parent
shared=base/'browser-extension'/'shared'
output=base/'target'/'browser-extensions'
output.mkdir(parents=True,exist_ok=True)
for kind in ['firefox','chromium']:
    folder=output/kind
    folder.mkdir(exist_ok=True)
    for p in shared.iterdir():
        shutil.copyfile(p,folder/p.name)
    manifest={
        'manifest_version':3,'name':'Blue Dragon Browser Bridge','version':'0.5.1',
        'description':'Pair a local Blue Dragon agent with a dedicated browser tab.',
        'permissions':['storage','tabs','scripting','webRequest'],
        'host_permissions':['<all_urls>'],
        'action':{'default_title':'Pair Blue Dragon'},
        'options_ui':{'page':'options.html','open_in_tab':True},
        'content_security_policy':{'extension_pages':"script-src 'self'; object-src 'none'; connect-src ws://127.0.0.1:* http://*:* https://*:*"},
    }
    if kind=='firefox':
        manifest['background']={'scripts':['page.js','background.js']}
        manifest['browser_specific_settings']={'gecko':{'id':'blue-dragon-bridge@khdertaysh1.github.io','strict_min_version':'140.0','data_collection_permissions':{'required':['websiteContent','browsingActivity','websiteActivity']}},'gecko_android':{'strict_min_version':'142.0'}}
        name='blue-dragon-browser-firefox.xpi'
    else:
        manifest['background']={'service_worker':'worker.js'}
        manifest['minimum_chrome_version']='116'
        name='blue-dragon-browser-chromium.zip'
    (folder/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    with zipfile.ZipFile(output/name,'w',zipfile.ZIP_DEFLATED) as archive:
        for p in sorted(folder.iterdir()):archive.write(p,p.name)
    print(output/name)
