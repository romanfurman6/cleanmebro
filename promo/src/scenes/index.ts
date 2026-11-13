import * as openerCss from './opener.css';
import * as mainCss from './main.css';
import {create as openerCreate} from './opener.draw';
import {create as mainCreate} from './main.draw';
import type {SceneModule} from '../Scene';
export const opener: SceneModule = {...openerCss, create: openerCreate};
export const main: SceneModule = {...mainCss, create: mainCreate};
