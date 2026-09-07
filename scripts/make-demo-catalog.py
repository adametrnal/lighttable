#!/usr/bin/env python3
"""Generate a synthetic Lighttable-only catalog and color-card JPEGs (needs Pillow).

This is a test fixture for Lighttable's reader, not a complete Lightroom catalog.
No personal photographs, catalogs or identifiers are read. Refuses existing output.
"""
import argparse
import colorsys
from pathlib import Path
import sqlite3
import uuid
from PIL import Image, ImageDraw


def create(root, count):
    root.mkdir(parents=True, exist_ok=False)
    photos = root/'photos'/'Color studies'
    photos.mkdir(parents=True)
    preview = root/'Synthetic Demo Previews.lrdata'
    preview.mkdir()
    db = sqlite3.connect(root/'Synthetic Demo.lrcat')
    db.executescript("""
    CREATE TABLE Adobe_variablesTable(name,value);
    INSERT INTO Adobe_variablesTable VALUES('Adobe_DBVersion','1504001');
    CREATE TABLE Adobe_images(id_local INTEGER,id_global,rootFile,fileFormat,captureTime,fileWidth,fileHeight,copyName,masterImage,orientation,rating,pick);
    CREATE TABLE AgLibraryFile(id_local INTEGER,id_global,baseName,extension,folder,sidecarExtensions);
    CREATE TABLE AgLibraryFolder(id_local INTEGER,pathFromRoot,rootFolder);
    CREATE TABLE AgLibraryRootFolder(id_local INTEGER,absolutePath,relativePathFromCatalog);
    CREATE TABLE AgHarvestedExifMetadata(image,aperture,shutterSpeed,isoSpeedRating,focalLength,cameraModelRef,lensRef);
    CREATE TABLE AgInternedExifCameraModel(id_local,value);
    CREATE TABLE AgInternedExifLens(id_local,value);
    CREATE TABLE AgLibraryIPTC(image,caption);
    CREATE TABLE AgLibraryKeyword(id_local,name);
    CREATE TABLE AgLibraryKeywordImage(image,tag);
    CREATE TABLE AgLibraryCollection(id_local,name,creationId,parent,systemOnly);
    CREATE TABLE AgLibraryCollectionImage(collection,image);
    CREATE TABLE AgLibraryCollectionContent(collection,content,owningModule);
    INSERT INTO AgLibraryRootFolder VALUES(1,'/synthetic/photos/','photos/');
    INSERT INTO AgLibraryFolder VALUES(1,'Color studies/',1);
    INSERT INTO AgInternedExifCameraModel VALUES(1,'Synthetic test imagery');
    INSERT INTO AgInternedExifLens VALUES(1,'Generated color cards');
    INSERT INTO AgLibraryCollection VALUES(10,'Demo collections','com.adobe.ag.library.group',NULL,0);
    INSERT INTO AgLibraryCollection VALUES(11,'Color studies','com.adobe.ag.library.collection',10,0);
    INSERT INTO AgLibraryCollection VALUES(12,'Four or five stars','com.adobe.ag.library.smart_collection',10,0);
    INSERT INTO AgLibraryKeyword VALUES(1,'synthetic demonstration');
    """)
    db.execute('INSERT INTO AgLibraryCollectionContent VALUES(12,?,?)',
               ("s={{criteria='rating',operation='>=',value=4},combine='intersect'}",
                'ag.library.smart_collection'))
    cache = sqlite3.connect(preview/'previews.db')
    cache.executescript('CREATE TABLE ImageCacheEntry(imageId,uuid,digest);'
                       'CREATE TABLE PyramidLevel(uuid,digest,longDimension);'
                       'CREATE TABLE Pyramid(uuid,digest,colorProfile);')
    for i in range(1, count+1):
        uid = str(uuid.uuid5(uuid.NAMESPACE_URL, f'https://example.invalid/lighttable/demo/{i}')).upper()
        file_uid = str(uuid.uuid5(uuid.NAMESPACE_URL, f'https://example.invalid/lighttable/file/{i}')).upper()
        color = tuple(round(v*255) for v in colorsys.hsv_to_rgb((i*0.0618)%1, .48, .80))
        im = Image.new('RGB', (720,480), color)
        draw = ImageDraw.Draw(im)
        draw.rectangle((24,24,696,456), outline='#faf2de', width=3)
        draw.ellipse((430,70,615,255), fill='#f0cd85')
        draw.polygon([(24,380),(210,140),(385,380)], fill='#253c50')
        draw.polygon([(230,380),(435,225),(696,380)], fill='#477775')
        draw.text((48,415), f'LIGHTTABLE   /   COLOR STUDY {i:03d}   /   SYNTHETIC FIXTURE', fill='#fff9ec')
        name = f'Color-study-{i:03d}'
        image = photos/f'{name}.jpg'
        im.save(image, quality=92)
        folder = preview/uid[0]/uid[:4]
        folder.mkdir(parents=True, exist_ok=True)
        (folder/f'{uid}-de00_720').write_bytes(image.read_bytes())
        db.execute('INSERT INTO AgLibraryFile VALUES(?,?,?,?,?,?)', (i,file_uid,name,'jpg',1,''))
        db.execute('INSERT INTO Adobe_images VALUES(?,?,?,?,?,?,?,?,?,?,?,?)',
                   (i,uid,i,'JPEG',f'2026-01-{1+(i-1)//24:02d}T{(i-1)%24:02d}:00:00',720,480,None,None,'AB',i%6,1 if i%7==0 else 0))
        db.execute('INSERT INTO AgHarvestedExifMetadata VALUES(?,?,?,?,?,?,?)',(i,4,7,100,50,1,1))
        db.execute('INSERT INTO AgLibraryIPTC VALUES(?,?)',(i,'Generated test card. No personal photograph.'))
        db.execute('INSERT INTO AgLibraryKeywordImage VALUES(?,1)',(i,))
        db.execute('INSERT INTO AgLibraryCollectionImage VALUES(11,?)',(i,))
        cache.execute('INSERT INTO ImageCacheEntry VALUES(?,?,?)',(i,uid,'de00'))
        cache.execute('INSERT INTO PyramidLevel VALUES(?,?,720)',(uid,'de00'))
        cache.execute('INSERT INTO Pyramid VALUES(?,?,?)',(uid,'de00','sRGB'))
    db.commit(); cache.commit()
    db.close(); cache.close()
    (root/'README.txt').write_text('Synthetic Lighttable test fixture, generated by make-demo-catalog.py.\nNot a complete catalog for use in Adobe Lightroom.\nGenerated imagery is covered by the Lighttable MIT license.\n')
    return root/'Synthetic Demo.lrcat'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    parser.add_argument('--count', type=int, default=96)
    args = parser.parse_args()
    if not 32 <= args.count <= 672:
        parser.error('--count must be between 32 and 672')
    print(create(args.output.resolve(), args.count))
