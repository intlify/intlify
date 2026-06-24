import { resolve } from 'node:path'

import { calibrateBindings, writeBindingCalibration } from './binding-calibration.mjs'

const rootDir = resolve(import.meta.dirname, '..')
const bindings = await calibrateBindings({ rootDir })

await writeBindingCalibration(rootDir, bindings)
console.log('binding calibration complete')
