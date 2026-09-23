import React, { useState, useEffect } from 'react';

export const ReserveDashboard = () => {
  const [reserves, setReserves] = useState([]);
  
  useEffect(() => {
    // Fetch reserve stats and config from API
    // Example: fetch('/api/reserve/stats').then(res => setReserves(res.data));
  }, []);

  return (
    <div className="reserve-dashboard">
      <h1>Treasury & Reserve Management</h1>
      <p>Configure reserve factors and manage treasury withdrawals.</p>
      
      <div className="reserve-list">
        {reserves.length === 0 ? (
          <p>No reserves loaded</p>
        ) : (
          <ul>
            {reserves.map((reserve: any) => (
              <li key={reserve.asset}>
                {reserve.asset}: {reserve.balance} (Factor: {reserve.factor})
              </li>
            ))}
          </ul>
        )}
      </div>
      
      <div className="actions">
        <button onClick={() => alert('Withdraw to Treasury')}>Withdraw Reserves</button>
      </div>
    </div>
  );
};
