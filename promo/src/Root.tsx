import React from 'react';
import {Composition} from 'remotion';
import './fonts.css';
import {Promo16x9} from './Promo16x9';
import {Promo9x16} from './Promo9x16';
import {FPS, FRAMES} from './timing';

export const RemotionRoot: React.FC = () => (
  <>
    <Composition id="Promo16x9" component={Promo16x9} durationInFrames={FRAMES} fps={FPS} width={1920} height={1080} />
    <Composition id="Promo9x16" component={Promo9x16} durationInFrames={FRAMES} fps={FPS} width={1080} height={1920} />
  </>
);
