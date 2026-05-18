{ data, lib }:
let
  toTomlVal = v:
    if builtins.isString v then "\"${lib.escape ["\"" "\\"] v}\""
    else if builtins.isInt v then toString v
    else if builtins.isBool v then (if v then "true" else "false")
    else if builtins.isList v then "[ " + builtins.concatStringsSep ", " (builtins.map toTomlVal v) + " ]"
    else "\"\"";

  get = path: default:
    let
      parts = lib.splitString "." path;
      step = acc: key: if builtins.isAttrs acc && acc ? ${key} then acc.${key} else null;
      res = builtins.foldl' step data parts;
    in if res == null then default else res;

  maxDisc = builtins.foldl' (acc: t: lib.max acc (if t ? metadata && t.metadata ? discnumber then t.metadata.discnumber else 1)) 1 (data.tracks or []);

  renderAlbum = 
    "[album]\n" +
    "albumartist = ${toTomlVal (get "album.metadata.albumartist" "")}\n" +
    "album = ${toTomlVal (get "album.metadata.album" "")}\n" +
    "date = ${toTomlVal (get "album.metadata.date" "")}\n" +
    "genre = ${toTomlVal (get "album.metadata.genre" [])}\n" +
    "styles = ${toTomlVal (get "album.metadata.styles" [])}\n" +
    "original_date = ${toTomlVal (get "album.metadata.original_date" "")}\n" +
    "country = ${toTomlVal (get "album.metadata.country" "")}\n" +
    "label = ${toTomlVal (get "album.metadata.label" "")}\n" +
    "catalognumber = ${toTomlVal (get "album.metadata.catalognumber" "")}\n" +
    "release_date = ${toTomlVal (get "album.metadata.release_date" "")}\n" +
    "musicbrainz_albumid = ${toTomlVal (get "album.mbid.musicbrainz_albumid" "")}\n" +
    "musicbrainz_albumartistid = ${toTomlVal (get "album.mbid.musicbrainz_albumartistid" "")}\n" +
    "musicbrainz_releasegroupid = ${toTomlVal (get "album.mbid.musicbrainz_releasegroupid" "")}\n";

  renderTrack = t:
    "[[tracks]]\n" +
    (if maxDisc > 1 then "discnumber = ${toTomlVal (if t ? metadata && t.metadata ? discnumber then t.metadata.discnumber else 1)}\n" else "") +
    "tracknumber = ${toTomlVal (if t ? metadata && t.metadata ? tracknumber then t.metadata.tracknumber else 0)}\n" +
    "title = ${toTomlVal (if t ? metadata && t.metadata ? title then t.metadata.title else "Untitled")}\n" +
    "artist = ${toTomlVal (if t ? metadata && t.metadata ? artist then t.metadata.artist else "")}\n" +
    "musicbrainz_trackid = ${toTomlVal (if t ? mbid && t.mbid ? musicbrainz_trackid then t.mbid.musicbrainz_trackid else "")}\n" +
    "musicbrainz_releasetrackid = ${toTomlVal (if t ? mbid && t.mbid ? musicbrainz_releasetrackid then t.mbid.musicbrainz_releasetrackid else "")}\n" +
    "musicbrainz_artistid = ${toTomlVal (if t ? mbid && t.mbid ? musicbrainz_artistid then t.mbid.musicbrainz_artistid else "")}\n";

in
''
${renderAlbum}
${builtins.concatStringsSep "\n" (builtins.map renderTrack (data.tracks or []))}''
