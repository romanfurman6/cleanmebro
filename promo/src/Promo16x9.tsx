import React from 'react';
import {AbsoluteFill, Audio, staticFile, useCurrentFrame, useVideoConfig} from 'remotion';
import {Scene} from './Scene';
import {opener, main} from './scenes';
import {START} from './timing';

export const Promo16x9: React.FC = () => {
  const frame = useCurrentFrame();
  const {fps} = useVideoConfig();
  const t = frame / fps;
  return (
    <AbsoluteFill style={{background: '#000'}}>
      {t < START ? <Scene key="opener" mod={opener} t={t} /> : <Scene key="main" mod={main} t={t} />}
      <Audio src={staticFile('music.mp3')} />
    </AbsoluteFill>
  );
};
