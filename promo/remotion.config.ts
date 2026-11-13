import {Config} from '@remotion/cli/config';
Config.setVideoImageFormat('jpeg');
Config.setJpegQuality(95);
Config.setCodec('h264');
Config.setCrf(17);
Config.setOverwriteOutput(true);
// Optional: use a local Chrome headless shell instead of the one Remotion downloads.
if (process.env.REMOTION_CHROME) Config.setBrowserExecutable(process.env.REMOTION_CHROME);
