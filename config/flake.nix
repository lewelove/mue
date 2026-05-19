{
  description = "Munix Core Flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }: let
    system = "x86_64-linux";
    pkgs = nixpkgs.legacyPackages.${system};

  in {
    packages.${system} = {
      env = pkgs.symlinkJoin {
        name = "munix-env";
        paths = [
          pkgs.shntool
          pkgs.cuetools
          pkgs.flac
          pkgs.opus-tools
          pkgs.imagemagick
          pkgs.jq
          pkgs.stdenv
          nixpkgs
        ];
      };
    };

    lib = {
      albumNixTemplate = { data }: import ./album.nix.template.nix { inherit data; lib = pkgs.lib; };
      metadataTomlTemplate = { data }: import ./metadata.toml.template.nix { inherit data; lib = pkgs.lib; };

      evalConfig = albumArgs: let
        envOrigin = builtins.getEnv "MUNIX_ORIGIN_PATH";
        envSourceName = builtins.getEnv "MUNIX_SOURCE_NAME";
        
        origin = {
          path = if ((albumArgs.origin.path or "") != "") then albumArgs.origin.path else envOrigin;
          hash = albumArgs.origin.hash or "";
        };
        
        source = {
          torrent = {
            file = albumArgs.source.torrent.file or "";
            name = if ((albumArgs.source.torrent.name or "") != "") then albumArgs.source.torrent.name else envSourceName;
            hash = albumArgs.source.torrent.hash or "";
          };
          web = {
            url = albumArgs.source.web.url or "";
            hash = albumArgs.source.web.hash or "";
          };
        };
      in builtins.scopedImport { inherit origin source; } ./config.nix;

      splitCueImage = { name ? "split", cue, image }: pkgs.stdenv.mkDerivation {
        inherit name cue image;
        buildInputs = [ pkgs.shntool pkgs.cuetools pkgs.flac pkgs.imagemagick pkgs.jq ];
        unpackPhase = "true";
        buildPhase = ''
          mkdir -p $out
          shnsplit -f "$cue" -o flac -t "%n" -d $out "$image"
        '';
        installPhase = "true";
      };

      mkCover = { name, src, relPath ? null }: pkgs.stdenv.mkDerivation {
        inherit name src relPath;
        buildInputs = [ pkgs.imagemagick ];
        unpackPhase = "true";
        buildPhase = ''
          INPUT_FILE="${if relPath == null then "$src" else "$src/$relPath"}"
          magick "$INPUT_FILE" -filter Mitchell -thumbnail 1080x1080^ -gravity center -extent 1080x1080 cover.png
        '';
        installPhase = ''
          mkdir -p $out
          cp cover.png $out/cover.png
        '';
      };

      mkFlacTrack = { name, src, relPath, tags ? [], cover ? null }: pkgs.stdenv.mkDerivation {
        inherit name src relPath;
        buildInputs = [ pkgs.flac ];
        unpackPhase = "true";
        buildPhase = ''
          cp "$src/$relPath" track.flac
          chmod +w track.flac
          
          metaflac --remove-all-tags \
            ${if cover != null then "--import-picture-from=\"${cover}/cover.png\"" else ""} \
            ${pkgs.lib.concatMapStringsSep " " (t: "--set-tag=\"${pkgs.lib.escape ["\"" "\\" "$"] t}\"") tags} \
            track.flac
        '';
        installPhase = ''
          mkdir -p $out
          cp track.flac $out/track.flac
        '';
      };

      mkOpusTrack = { name, src, relPath, kbps ? 128, tags ? [], cover ? null }: pkgs.stdenv.mkDerivation {
        inherit name src relPath kbps;
        buildInputs = [ pkgs.opus-tools ];
        unpackPhase = "true";
        buildPhase = ''
          cp "$src/$relPath" track.flac
          chmod +w track.flac
          
          opusenc --bitrate "$kbps" \
            ${if cover != null then "--picture \"${cover}/cover.png\"" else ""} \
            ${pkgs.lib.concatMapStringsSep " " (t: "--comment \"${pkgs.lib.escape ["\"" "\\" "$"] t}\"") tags} \
            track.flac track.opus
        '';
        installPhase = ''
          mkdir -p $out
          cp track.opus $out/track.opus
        '';
      };

      mkAlbum = args@{ 
        name, 
        origin ? { path = ""; hash = ""; },
        source ? {},
        album ? { info = {}; metadata = {}; mbid = {}; },
        tracks ? [], 
        cover ? null
      }: let
        config = self.lib.evalConfig args;

        resolvePath = data: pathStr: let
          parts = pkgs.lib.splitString "." pathStr;
        in pkgs.lib.foldl' (acc: attr: if builtins.isAttrs acc && acc ? ${attr} then acc.${attr} else null) data parts;

        trackContexts = pkgs.lib.lists.imap1 (idx: track: let
          dataCtx = { inherit album; tracks = track; };
          
          t_disc = resolvePath dataCtx "tracks.metadata.discnumber";
          a_disc = resolvePath dataCtx "album.metadata.discnumber";
          disc = if t_disc != null then t_disc else if a_disc != null then a_disc else 1;

          t_trk = resolvePath dataCtx "tracks.metadata.tracknumber";
          a_trk = resolvePath dataCtx "album.metadata.tracknumber";
          trk = if t_trk != null then t_trk else if a_trk != null then a_trk else 0;

          t_title = resolvePath dataCtx "tracks.metadata.title";
          a_title = resolvePath dataCtx "album.metadata.title";
          title = if t_title != null then t_title else if a_title != null then a_title else "Untitled";

          resolvedTags = pkgs.lib.concatLists (pkgs.lib.mapAttrsToList (tag: pathStr: 
            let 
              val = resolvePath dataCtx pathStr;
            in
            if val == null || val == "" || val == [] then []
            else if builtins.isList val then builtins.map (v: "${tag}=${toString v}") val
            else [ "${tag}=${toString val}" ]
          ) (config.writeFlacKeys or {}));

          resolvedOpusTags = pkgs.lib.concatLists (pkgs.lib.mapAttrsToList (tag: pathStr: 
            let 
              val = resolvePath dataCtx pathStr;
            in
            if val == null || val == "" || val == [] then []
            else if builtins.isList val then builtins.map (v: "${tag}=${toString v}") val
            else [ "${tag}=${toString val}" ]
          ) (config.writeOpusKeys or {}));

        in { inherit track disc trk title resolvedTags resolvedOpusTags; }) tracks;

        maxDisc = builtins.foldl' (acc: t: pkgs.lib.max acc t.disc) 1 trackContexts;
        maxTrack = builtins.foldl' (acc: t: pkgs.lib.max acc t.trk) 1 trackContexts;

        trackIds = builtins.map (t: "${toString t.disc}-${toString t.trk}") trackContexts;
        uniqueTrackIds = pkgs.lib.unique trackIds;
        hasDuplicates = builtins.length trackIds != builtins.length uniqueTrackIds;
        
        discPadLen = builtins.stringLength (toString maxDisc);
        trackPadLen = pkgs.lib.max 2 (builtins.stringLength (toString maxTrack));

        metadataToml = pkgs.writeText "metadata.toml" (self.lib.metadataTomlTemplate { data = args; });

        envOrigin = builtins.getEnv "MUNIX_ORIGIN_PATH";
        envSanitizedSourceName = builtins.getEnv "MUNIX_SANITIZED_SOURCE_NAME";
        srcBaseName = if envSanitizedSourceName != "" then envSanitizedSourceName else name;
        
        resolvedOriginPath = 
          if (origin.path or "") != "" then origin.path 
          else if envOrigin != "" then envOrigin
          else config.origin;

        rawSrcPath = if resolvedOriginPath != "" then resolvedOriginPath
                     else throw "Origin path missing in album.nix, config.nix, and environment";

        realSrc = 
          let
            isStore = builtins.isString rawSrcPath && builtins.match "/nix/store/[0-9abcdfghijklmnpqrsvwxyz]{32}-.*" rawSrcPath != null;
          in
          if isStore then 
            builtins.storePath (builtins.elemAt (builtins.match ".*(/nix/store/[0-9abcdfghijklmnpqrsvwxyz]{32}-.*)" rawSrcPath) 0)
          else
            let
              pathObj = if builtins.isPath rawSrcPath then rawSrcPath
                        else if builtins.isString rawSrcPath && pkgs.lib.hasPrefix "/" rawSrcPath then /. + rawSrcPath
                        else rawSrcPath;
            in
            if builtins.isPath pathObj then
              builtins.path { 
                name = "${srcBaseName}-source"; 
                path = pathObj; 
                sha256 = origin.hash; 
              }
            else 
              pathObj;

        processedCover = if cover != null && cover.file != null
                         then (
                           if builtins.isPath cover.file
                           then let
                             realCoverPath = if (cover.hash or "") != "" then
                               builtins.path { 
                                 name = "${name}-cover-src"; 
                                 path = cover.file; 
                                 sha256 = cover.hash; 
                                 recursive = false;
                               }
                             else cover.file;
                           in self.lib.mkCover { name = "${name}-cover"; src = realCoverPath; }
                           else self.lib.mkCover { name = "${name}-cover"; src = realSrc; relPath = cover.file; }
                         )
                         else null;
        
        opusKbps = config.library.opus.kbps or 128;

        mkOutputDerivation = ext: buildTracks: pkgs.stdenv.mkDerivation {
          name = "${name}-${ext}";
          src = realSrc;
          passthru = {
            sourceStorePath = realSrc;
            rawArgs = args;
          };
          unpackPhase = "true";
          buildPhase = ''
            mkdir -p $out
            ${pkgs.lib.strings.concatMapStringsSep "\n" (t: ''
              ln -s "${t.drv}/track.${ext}" "$out/${t.fileName}"
            '') buildTracks}
            cp ${metadataToml} $out/metadata.toml
          '';
          installPhase = "true";
        };

        flacTracks = builtins.map (tc: let
          discStr = pkgs.lib.fixedWidthString discPadLen "0" (toString tc.disc);
          trkStr = pkgs.lib.fixedWidthString trackPadLen "0" (toString tc.trk);
          fileName = if maxDisc == 1 then "${trkStr} - ${tc.title}.flac" else "${discStr}.${trkStr} - ${tc.title}.flac";
        in {
          inherit fileName;
          drv = self.lib.mkFlacTrack {
            name = "${name}-flac-disc${toString tc.disc}-track${toString tc.trk}";
            src = realSrc;
            relPath = tc.track.file;
            tags = tc.resolvedTags;
            cover = processedCover;
          };
        }) trackContexts;

        opusTracks = builtins.map (tc: let
          discStr = pkgs.lib.fixedWidthString discPadLen "0" (toString tc.disc);
          trkStr = pkgs.lib.fixedWidthString trackPadLen "0" (toString tc.trk);
          fileName = if maxDisc == 1 then "${trkStr} - ${tc.title}.opus" else "${discStr}.${trkStr} - ${tc.title}.opus";
        in {
          inherit fileName;
          drv = self.lib.mkOpusTrack {
            name = "${name}-opus-disc${toString tc.disc}-track${toString tc.trk}";
            src = realSrc;
            relPath = tc.track.file;
            tags = tc.resolvedOpusTags;
            cover = processedCover;
            kbps = opusKbps;
          };
        }) trackContexts;

      in if hasDuplicates then throw "Duplicate discnumber and tracknumber combinations found in tracks." else pkgs.lib.optionalAttrs (config.library.flac.enable or false) {
        flac = mkOutputDerivation "flac" flacTracks;
      } // pkgs.lib.optionalAttrs (config.library.opus.enable or false) {
        opus = mkOutputDerivation "opus" opusTracks;
      };
    };
  };
}
