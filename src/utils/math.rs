use rand::Rng;
use rand_distr::{Dirichlet, Distribution};

pub fn get_dirichlet_distributed_amount(alpha: u64, n: usize) -> Vec<u64> {
    if n == 0 {
        return vec![];
    }

    let mut rng = rand::thread_rng();
    let proportions = if n > 1 {
        Dirichlet::new(&*vec![1.0; n]).unwrap().sample(&mut rng)
    } else {
        vec![1.0]
    };

    let mut amounts: Vec<u64> = proportions
        .iter()
        .map(|&x| (x * alpha as f64).round() as u64)
        .collect();

    // Adjust for rounding errors to ensure the sum equals `alpha`
    let total: u64 = amounts.iter().sum();
    if total != alpha {
        let difference = alpha as i64 - total as i64;
        if difference > 0 {
            // Add the difference to the first element
            amounts[0] += difference as u64;
        } else if difference < 0 {
            // Subtract the difference from the first element
            amounts[0] -= difference.abs() as u64;
        }
    }

    amounts
}

pub fn get_dirichlet_distributed_with_min_amount(
    alpha: u64,
    n: usize,
    min_amount: u64,
) -> Vec<u64> {
    if n == 0 {
        return vec![];
    }
    if n == 1 {
        return vec![alpha];
    }

    // Check if the total alpha is less than the minimum required sum
    let min_required_total = n as u64 * min_amount;
    if alpha < min_required_total {
        // If alpha is too small to satisfy the minimum amount for each, return min_amount for all
        return vec![min_amount; n];
    }

    let remaining_alpha = alpha - min_required_total;
    let mut rng = rand::thread_rng();

    // Generate proportions using Dirichlet distribution
    let proportions = Dirichlet::new(&*vec![1.0; n]).unwrap().sample(&mut rng);
        
    // Distribute the remaining alpha based on the proportions
    let mut amounts: Vec<u64> = proportions
        .iter()
        .map(|&x| (x * remaining_alpha as f64).round() as u64 + min_amount)
        .collect();

    // Adjust for rounding errors to ensure the sum equals `alpha`
    let total: u64 = amounts.iter().sum();
    if total != alpha {
        let difference = alpha as i64 - total as i64;
        if difference > 0 {
            amounts[0] += difference as u64;
        } else if difference < 0 {
            amounts[0] -= difference.abs() as u64;
        }
    }

    amounts
}
