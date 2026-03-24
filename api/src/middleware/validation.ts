import { body, param, validationResult, check } from 'express-validator';
import { Request, Response, NextFunction } from 'express';
import { ValidationError } from '../utils/errors';
import { StrKey } from '@stellar/stellar-sdk';

const VALID_OPERATIONS = ['deposit', 'borrow', 'repay', 'withdraw'];

export const validateRequest = (req: Request, res: Response, next: NextFunction) => {
  const errors = validationResult(req);
  if (!errors.isEmpty()) {
    const errorMessages = errors
      .array()
      .map((err) => err.msg)
      .join(', ');
    throw new ValidationError(errorMessages);
  }
  next();
};

export const amountValidation = [
  check('amount')
    .notEmpty()
    .withMessage('Amount is required')
    .isFloat({ min: 0.0000001 })
    .withMessage('Amount must be greater than 0'),
];

export const prepareValidation = [
  param('operation')
    .isIn(VALID_OPERATIONS)
    .withMessage(`Operation must be one of: ${VALID_OPERATIONS.join(', ')}`),
  check('userAddress')
    .notEmpty()
    .withMessage('User address is required')
    .custom((value) => {
      if (!StrKey.isValidEd25519PublicKey(value)) {
        throw new Error('Invalid Stellar address');
      }
      return true;
    }),
  ...amountValidation,
  check('assetAddress').optional().isString().notEmpty().withMessage('Asset address is required'),
  validateRequest,
];

export const submitValidation = [
  body('signedXdr').isString().notEmpty().withMessage('signedXdr is required'),
  validateRequest,
];

// Kept for backward compatibility — deprecated, will be removed in v2
export const depositValidation = prepareValidation;
export const borrowValidation = prepareValidation;
export const repayValidation = prepareValidation;
export const withdrawValidation = prepareValidation;
